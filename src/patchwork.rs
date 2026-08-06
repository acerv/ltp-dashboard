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

use crate::config::PatchworkInstance;
use isahc::config::{Configurable, DnsCache, VersionNegotiation};
use isahc::prelude::*;
use isahc::HttpClient;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

// States: new, under-review, needs-review-ack (numeric ID 11), and needs-ack
const STATES: &[&str] = &["new", "under-review", "11", "needs-ack"];

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

pub fn build_client(instance: &PatchworkInstance) -> anyhow::Result<HttpClient> {
    Ok(HttpClient::builder()
        .version_negotiation(VersionNegotiation::http2())
        .max_connections(instance.max_connections)
        .max_connections_per_host(instance.max_connections_per_host)
        .connection_cache_size(instance.max_connections)
        .tcp_nodelay()
        .tcp_keepalive(Duration::from_secs(60))
        .dns_cache(DnsCache::Forever)
        .timeout(Duration::from_secs(instance.timeout_secs))
        .build()?)
}

async fn fetch_json<T: DeserializeOwned + Unpin>(
    client: &HttpClient,
    url: &str,
) -> anyhow::Result<T> {
    let mut r = client.get_async(url).await?;
    if !r.status().is_success() {
        anyhow::bail!("HTTP {}", r.status());
    }
    let data: T = r.json().await?;
    Ok(data)
}

async fn fetch_text(client: &HttpClient, url: &str) -> anyhow::Result<String> {
    let mut r = client.get_async(url).await?;
    if !r.status().is_success() {
        anyhow::bail!("HTTP {}", r.status());
    }
    let text = r.text().await?;
    Ok(text)
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Submitter {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesRef {
    pub id: Option<u64>,
    pub total: Option<u32>,
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Tags {
    #[serde(rename = "reviewed-by-count", default)]
    pub reviewed_by_count: u32,
    #[serde(rename = "acked-by-count", default)]
    pub acked_by_count: u32,
    #[serde(rename = "signed-off-by-count", default)]
    pub signed_off_by_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Delegate {
    pub id: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawPatch {
    pub id: u64,
    pub name: String,
    pub date: String,
    pub state: String,
    pub submitter: Submitter,
    #[serde(default)]
    pub tags: Tags,
    #[serde(default)]
    pub series: Vec<SeriesRef>,
    pub delegate: Option<Delegate>,
    pub web_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PagedResponse {
    count: Option<usize>,
    results: Vec<RawPatch>,
}

// ---------------------------------------------------------------------------
// Patch list fetching
// ---------------------------------------------------------------------------

/// Fetch all patches for all configured states, up to max_patches total.
/// Returns (patches, counts_per_state).
pub async fn fetch_all_patches(
    client: &HttpClient,
    instance: &PatchworkInstance,
    max_patches: usize,
) -> anyhow::Result<(Vec<RawPatch>, HashMap<String, usize>)> {
    let state_display = HashMap::from([("11".to_string(), "needs-review-ack".to_string())]);

    let futs: Vec<_> = STATES
        .iter()
        .map(|&state| async move {
            let result = fetch_state_patches(client, instance, state, max_patches).await;
            (state, result)
        })
        .collect();

    let results = futures::future::join_all(futs).await;

    let mut all: Vec<RawPatch> = Vec::new();
    let mut counts = HashMap::new();

    for (state, result) in results {
        let display = state_display
            .get(state)
            .cloned()
            .unwrap_or_else(|| state.to_string());
        match result {
            Ok(patches) => {
                counts.insert(display, patches.len());
                all.extend(patches);
            }
            Err(e) => eprintln!("Warning: failed to fetch state {state}: {e}"),
        }
    }

    all.truncate(max_patches);
    Ok((all, counts))
}

async fn fetch_page(client: &HttpClient, url: &str) -> anyhow::Result<Vec<RawPatch>> {
    let text = fetch_text(client, url).await?;
    if text.trim_start().starts_with('[') {
        Ok(serde_json::from_str(&text)?)
    } else {
        Ok(serde_json::from_str::<PagedResponse>(&text)?.results)
    }
}

async fn fetch_state_patches(
    client: &HttpClient,
    instance: &PatchworkInstance,
    state: &str,
    max_patches: usize,
) -> anyhow::Result<Vec<RawPatch>> {
    let base_url = format!(
        "{}/patches/?project={}&state={state}&order=-date&per_page=100",
        instance.url, instance.project
    );

    let text = fetch_text(client, &base_url).await?;

    let (mut all, total) = if text.trim_start().starts_with('[') {
        let results: Vec<RawPatch> = serde_json::from_str(&text)?;
        (results, None)
    } else {
        let paged: PagedResponse = serde_json::from_str(&text)?;
        (paged.results, paged.count)
    };

    if let Some(total) = total {
        let n_pages = total.div_ceil(100);
        if n_pages > 1 {
            let futs: Vec<_> = (2..=n_pages)
                .map(|page| {
                    let url = format!("{base_url}&page={page}");
                    async move { fetch_page(client, &url).await }
                })
                .collect();

            let results = futures::future::join_all(futs).await;
            for result in results {
                match result {
                    Ok(patches) => all.extend(patches),
                    Err(e) => eprintln!("Warning: page fetch failed: {e}"),
                }
            }
        }
    }

    all.truncate(max_patches);
    Ok(all)
}

// ---------------------------------------------------------------------------
// Comments (email replies) — Reviewed-by / Acked-by detection
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CommentEntry {
    content: String,
}

#[derive(Debug, Deserialize)]
struct CommentsResponse {
    results: Vec<CommentEntry>,
}

/// Fetch email reply counts for all patches in parallel.
pub async fn fetch_all_comment_tags(
    client: &HttpClient,
    instance: &PatchworkInstance,
    patch_ids: &[u64],
) -> HashMap<u64, (u32, u32)> {
    let tasks: Vec<_> = patch_ids
        .iter()
        .map(|&id| async move {
            let result = fetch_comment_tags_for_patch(client, instance, id).await;
            (id, result)
        })
        .collect();

    let results = futures::future::join_all(tasks).await;
    let mut out = HashMap::with_capacity(patch_ids.len());
    for (id, result) in results {
        match result {
            Ok(counts) => {
                out.insert(id, counts);
            }
            Err(e) => eprintln!("Warning: comments fetch failed for patch {id}: {e}"),
        }
    }
    out
}

async fn fetch_comment_tags_for_patch(
    client: &HttpClient,
    instance: &PatchworkInstance,
    patch_id: u64,
) -> anyhow::Result<(u32, u32)> {
    let url = format!("{}/patches/{patch_id}/comments/", instance.url);
    let text = fetch_text(client, &url).await?;

    let comments: Vec<CommentEntry> = if text.trim_start().starts_with('[') {
        serde_json::from_str(&text)?
    } else {
        serde_json::from_str::<CommentsResponse>(&text)?.results
    };

    let mut reviewed = 0u32;
    let mut acked = 0u32;
    for c in &comments {
        for line in c.content.lines() {
            let l = line.trim();
            if l.starts_with("Reviewed-by:") {
                reviewed += 1;
            } else if l.starts_with("Acked-by:") {
                acked += 1;
            }
        }
    }
    Ok((reviewed, acked))
}

// ---------------------------------------------------------------------------
// Diff size
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PatchDetail {
    diff: Option<String>,
}

/// Fetch diff sizes (changed lines) for all patches in parallel.
pub async fn fetch_all_diff_sizes(
    client: &HttpClient,
    instance: &PatchworkInstance,
    patch_ids: &[u64],
) -> HashMap<u64, u32> {
    let tasks: Vec<_> = patch_ids
        .iter()
        .map(|&id| async move {
            let result = fetch_diff_lines_for_patch(client, instance, id).await;
            (id, result)
        })
        .collect();

    let results = futures::future::join_all(tasks).await;
    let mut out = HashMap::with_capacity(patch_ids.len());
    for (id, result) in results {
        match result {
            Ok(lines) => {
                out.insert(id, lines);
            }
            Err(e) => eprintln!("Warning: diff fetch failed for patch {id}: {e}"),
        }
    }
    out
}

async fn fetch_diff_lines_for_patch(
    client: &HttpClient,
    instance: &PatchworkInstance,
    patch_id: u64,
) -> anyhow::Result<u32> {
    let url = format!("{}/patches/{patch_id}/", instance.url);
    let detail: PatchDetail = fetch_json(client, &url).await?;
    let lines = detail.diff.as_deref().map(count_diff_lines).unwrap_or(0);
    Ok(lines)
}

fn count_diff_lines(diff: &str) -> u32 {
    diff.lines()
        .filter(|l| {
            (l.starts_with('+') || l.starts_with('-'))
                && !l.starts_with("+++")
                && !l.starts_with("---")
        })
        .count() as u32
}

// ---------------------------------------------------------------------------
// CI checks
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CheckEntry {
    state: String,
}

#[derive(Debug, Deserialize)]
struct ChecksResponse {
    results: Vec<CheckEntry>,
}

/// Fetch CI check results for all patches in parallel.
pub async fn fetch_all_checks(
    client: &HttpClient,
    instance: &PatchworkInstance,
    patch_ids: &[u64],
) -> HashMap<u64, (u32, u32, u32)> {
    let tasks: Vec<_> = patch_ids
        .iter()
        .map(|&id| async move {
            let result = fetch_checks_for_patch(client, instance, id).await;
            (id, result)
        })
        .collect();

    let results = futures::future::join_all(tasks).await;
    let mut out = HashMap::with_capacity(patch_ids.len());
    for (id, result) in results {
        match result {
            Ok(counts) => {
                out.insert(id, counts);
            }
            Err(e) => eprintln!("Warning: checks fetch failed for patch {id}: {e}"),
        }
    }
    out
}

async fn fetch_checks_for_patch(
    client: &HttpClient,
    instance: &PatchworkInstance,
    patch_id: u64,
) -> anyhow::Result<(u32, u32, u32)> {
    let url = format!("{}/patches/{patch_id}/checks/", instance.url);
    let text = fetch_text(client, &url).await?;

    let results: Vec<CheckEntry> = if text.trim_start().starts_with('[') {
        serde_json::from_str(&text)?
    } else {
        serde_json::from_str::<ChecksResponse>(&text)?.results
    };

    let total = results.len() as u32;
    let passed = results.iter().filter(|c| c.state == "success").count() as u32;
    let failed = results.iter().filter(|c| c.state == "fail").count() as u32;
    Ok((passed, failed, total))
}
