// SPDX-License-Identifier: GPL-3.0-or-later
/*
 * Copyright (C) 2026 LTP - Linux Test Project
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

mod config;
mod patchwork;
mod scoring;
mod templates;
mod terminal;

use crate::config::PatchworkInstance;
use isahc::HttpClient;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::Local;
use clap::Parser;
use tokio::sync::RwLock;
use tower_http::compression::CompressionLayer;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "ltp-dashboard", about = "LTP patch dashboard")]
struct Cli {
    /// Expose a web interface instead of printing to terminal
    #[arg(long)]
    web: bool,

    /// Port for the web server (only used with --web)
    #[arg(long, default_value = "3030")]
    port: u16,

    /// Maximum number of patches to fetch
    #[arg(long, default_value = "500")]
    max: usize,

    /// Fetch CI check results for each patch (parallel)
    #[arg(long)]
    checks: bool,
}

// ---------------------------------------------------------------------------
// Shared state (web mode)
// ---------------------------------------------------------------------------

struct InstanceContext {
    instance: PatchworkInstance,
    client: HttpClient,
}

struct AppState {
    contexts: Vec<InstanceContext>,
    max_patches: usize,
    fetch_checks: bool,
    cache: RwLock<Option<(String, Instant)>>,
    refreshing: AtomicBool,
}

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);

// ---------------------------------------------------------------------------
// Shared fetch + score logic
// ---------------------------------------------------------------------------

async fn fetch_and_score(
    contexts: &[InstanceContext],
    max_patches: usize,
    fetch_checks: bool,
) -> anyhow::Result<(Vec<scoring::ScoredPatch>, HashMap<String, usize>)> {
    let mut all_scored = Vec::new();
    let mut all_counts = HashMap::new();

    let futs: Vec<_> = contexts.iter().map(|ctx| async move {
        let (raw, counts) = patchwork::fetch_all_patches(&ctx.client, &ctx.instance, max_patches).await?;

        let raw: Vec<_> = raw
            .into_iter()
            .filter(|p| !scoring::is_cover(&p.name))
            .collect();

        let mut scored: Vec<scoring::ScoredPatch> = raw.iter().map(|p| scoring::score_patch(p, &ctx.instance)).collect();

        let ids: Vec<u64> = scored.iter().map(|p| p.id).collect();

        // Always fetch comments and diff sizes in parallel; optionally fetch CI checks.
        let comments_fut = patchwork::fetch_all_comment_tags(&ctx.client, &ctx.instance, &ids);
        let diffs_fut = patchwork::fetch_all_diff_sizes(&ctx.client, &ctx.instance, &ids);
        if fetch_checks {
            eprintln!(
                "[{}] Fetching CI checks, comments and diffs for {} patches…",
                ctx.instance.project, scored.len()
            );
            let (comment_tags, diff_sizes, checks) = tokio::join!(
                comments_fut,
                diffs_fut,
                patchwork::fetch_all_checks(&ctx.client, &ctx.instance, &ids),
            );
            for p in &mut scored {
                if let Some(&(r, a)) = comment_tags.get(&p.id) {
                    p.reviewed += r;
                    p.acked += a;
                }
                if let Some(&lines) = diff_sizes.get(&p.id) {
                    p.diff_lines = lines;
                    let pts = scoring::score_diff_lines(lines);
                    if pts != 0 {
                        p.score += pts;
                        p.reasons.push(format!("small-diff:{lines}(+{pts})"));
                        let (tier, tier_label) = scoring::classify(p.score);
                        p.tier = tier;
                        p.tier_label = tier_label;
                    }
                }
                if let Some(&(passed, failed, total)) = checks.get(&p.id) {
                    p.checks_passed = passed;
                    p.checks_failed = failed;
                    p.checks_total = total;
                }
            }
        } else {
            eprintln!("[{}] Fetching comments and diffs for {} patches…", ctx.instance.project, scored.len());
            let (comment_tags, diff_sizes) = tokio::join!(comments_fut, diffs_fut);
            for p in &mut scored {
                if let Some(&(r, a)) = comment_tags.get(&p.id) {
                    p.reviewed += r;
                    p.acked += a;
                }
                if let Some(&lines) = diff_sizes.get(&p.id) {
                    p.diff_lines = lines;
                    let pts = scoring::score_diff_lines(lines);
                    if pts != 0 {
                        p.score += pts;
                        p.reasons.push(format!("small-diff:{lines}(+{pts})"));
                        let (tier, tier_label) = scoring::classify(p.score);
                        p.tier = tier;
                        p.tier_label = tier_label;
                    }
                }
            }
        }

        Ok::<_, anyhow::Error>((scored, counts))
    }).collect();

    let results = futures::future::join_all(futs).await;

    for res in results {
        let (scored, counts) = res?;
        all_scored.extend(scored);
        for (k, v) in counts {
            *all_counts.entry(k).or_insert(0) += v;
        }
    }

    all_scored.sort_by(|a, b| b.score.cmp(&a.score).then(b.days.cmp(&a.days)));
    scoring::mark_superseded(&mut all_scored);
    all_scored.retain(|p| !p.superseded);

    Ok((all_scored, all_counts))
}

// ---------------------------------------------------------------------------
// Terminal mode
// ---------------------------------------------------------------------------

async fn run_terminal(
    contexts: &[InstanceContext],
    max_patches: usize,
    fetch_checks: bool,
) -> anyhow::Result<()> {
    eprintln!("Fetching patches from Patchwork…");
    let (scored, counts) = fetch_and_score(contexts, max_patches, fetch_checks).await?;
    terminal::print_queue(&scored, &counts, fetch_checks);
    Ok(())
}

// ---------------------------------------------------------------------------
// Web mode
// ---------------------------------------------------------------------------

async fn index_handler(State(state): State<Arc<AppState>>) -> Response {
    let cached = {
        let cache = state.cache.read().await;
        cache
            .as_ref()
            .map(|(html, ts)| (html.clone(), ts.elapsed()))
    };

    match cached {
        // Fresh cache — serve immediately
        Some((html, elapsed)) if elapsed < CACHE_TTL => return html_response(html),

        // Stale cache — serve stale content, kick off background refresh
        Some((html, _)) => {
            if !state.refreshing.swap(true, Ordering::AcqRel) {
                let state2 = state.clone();
                tokio::spawn(async move {
                    match build_page(&state2.contexts, state2.max_patches, state2.fetch_checks).await
                    {
                        Ok(fresh) => {
                            let mut cache = state2.cache.write().await;
                            *cache = Some((fresh, Instant::now()));
                        }
                        Err(e) => eprintln!("Background refresh failed: {e}"),
                    }
                    state2.refreshing.store(false, Ordering::Release);
                });
            }
            return html_response(html);
        }

        // No cache yet — wait for prefetch or build synchronously
        None => {}
    }

    // If a prefetch is already running, poll until it finishes
    if state.refreshing.load(Ordering::Acquire) {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if !state.refreshing.load(Ordering::Acquire) {
                break;
            }
        }
        let cache = state.cache.read().await;
        if let Some((html, _)) = cache.as_ref() {
            return html_response(html.clone());
        }
    }

    match build_page(&state.contexts, state.max_patches, state.fetch_checks).await {
        Ok(html) => {
            let mut cache = state.cache.write().await;
            *cache = Some((html.clone(), Instant::now()));
            html_response(html)
        }
        Err(e) => {
            eprintln!("Error building page: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch patch data: {e}"),
            )
                .into_response()
        }
    }
}

fn html_response(html: String) -> Response {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}

async fn build_page(
    contexts: &[InstanceContext],
    max_patches: usize,
    fetch_checks: bool,
) -> anyhow::Result<String> {
    eprintln!("Fetching patches from Patchwork…");
    let (scored, _counts) = fetch_and_score(contexts, max_patches, fetch_checks).await?;
    eprintln!("Rendering {} patches…", scored.len());

    let generated_at = Local::now().format("%Y-%m-%d %H:%M:%S %Z").to_string();
    let projects = contexts.iter().map(|c| c.instance.display_name().to_string()).collect::<Vec<_>>().join(", ");
    let data = templates::TemplateData {
        patches: &scored,
        generated_at,
        show_checks: fetch_checks,
        projects,
    };

    Ok(templates::render_index(&data)?)
}

async fn run_web(
    contexts: Vec<InstanceContext>,
    port: u16,
    max_patches: usize,
    fetch_checks: bool,
) -> anyhow::Result<()> {
    let state = Arc::new(AppState {
        contexts,
        max_patches,
        fetch_checks,
        cache: RwLock::new(None),
        refreshing: AtomicBool::new(true),
    });

    // Warm the cache before the first request arrives
    {
        let state2 = state.clone();
        tokio::spawn(async move {
            eprintln!("Prefetching patches…");
            match build_page(&state2.contexts, state2.max_patches, state2.fetch_checks).await {
                Ok(html) => {
                    let mut cache = state2.cache.write().await;
                    *cache = Some((html, Instant::now()));
                    eprintln!("Cache warm.");
                }
                Err(e) => eprintln!("Prefetch failed: {e}"),
            }
            state2.refreshing.store(false, Ordering::Release);
        });
    }

    let app = Router::new()
        .route("/", get(index_handler))
        .layer(CompressionLayer::new())
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::Config::load()?;
    let cli = Cli::parse();

    // CLI args override config file values where explicitly supplied.
    let port = if cli.port != 3030 { cli.port } else { cfg.port };
    let max_patches = if cli.max != 500 {
        cli.max
    } else {
        cfg.max_patches
    };
    let fetch_checks = cli.checks || cfg.checks;

    let mut contexts = Vec::new();
    for instance in cfg.instances {
        let client = patchwork::build_client(&instance)?;
        contexts.push(InstanceContext { instance, client });
    }

    if cli.web {
        run_web(contexts, port, max_patches, fetch_checks).await
    } else {
        run_terminal(&contexts, max_patches, fetch_checks).await
    }
}
