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

use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

const PATCHWORK_BASE: &str = "https://patchwork.ozlabs.org/api";
const PROJECT: &str = "ltp";

// States: new, under-review, and needs-review-ack (numeric ID 11)
const STATES: &[&str] = &["new", "under-review", "11"];

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Submitter {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesRef {
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

/// Fetch all patches for all configured states, up to max_patches total.
/// Returns (patches, counts_per_state).
pub async fn fetch_all_patches(
    client: &Client,
    max_patches: usize,
) -> anyhow::Result<(Vec<RawPatch>, std::collections::HashMap<String, usize>)> {
    // Map numeric filter ID back to display name
    let state_display =
        std::collections::HashMap::from([("11".to_string(), "needs-review-ack".to_string())]);

    let mut handles = Vec::new();
    for &state in STATES {
        let client = client.clone();
        let state = state.to_owned();
        let max = max_patches;
        handles.push(tokio::spawn(async move {
            fetch_state_patches(&client, &state, max).await
        }));
    }

    let mut all: Vec<RawPatch> = Vec::new();
    let mut counts = std::collections::HashMap::new();

    for (handle, &state) in handles.into_iter().zip(STATES.iter()) {
        let display = state_display
            .get(state)
            .cloned()
            .unwrap_or_else(|| state.to_string());
        match handle.await {
            Ok(Ok(patches)) => {
                counts.insert(display, patches.len());
                all.extend(patches);
            }
            Ok(Err(e)) => eprintln!("Warning: failed to fetch state {state}: {e}"),
            Err(e) => eprintln!("Warning: task panicked for state {state}: {e}"),
        }
    }

    all.truncate(max_patches);
    Ok((all, counts))
}

async fn fetch_page(client: &Client, url: &str) -> anyhow::Result<Vec<RawPatch>> {
    let text = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await?
        .text()
        .await?;

    if text.trim_start().starts_with('[') {
        Ok(serde_json::from_str(&text)?)
    } else {
        Ok(serde_json::from_str::<PagedResponse>(&text)?.results)
    }
}

async fn fetch_state_patches(
    client: &Client,
    state: &str,
    max_patches: usize,
) -> anyhow::Result<Vec<RawPatch>> {
    let base_url = format!(
        "{PATCHWORK_BASE}/patches/?project={PROJECT}&state={state}&order=-date&per_page=100"
    );

    // Fetch page 1 to learn the total count
    let text = client
        .get(&base_url)
        .header("Accept", "application/json")
        .send()
        .await?
        .text()
        .await?;

    let (mut all, total) = if text.trim_start().starts_with('[') {
        let results: Vec<RawPatch> = serde_json::from_str(&text)?;
        (results, None)
    } else {
        let paged: PagedResponse = serde_json::from_str(&text)?;
        (paged.results, paged.count)
    };

    // Spawn all remaining pages in parallel
    if let Some(total) = total {
        let n_pages = total.div_ceil(100);
        if n_pages > 1 {
            let handles: Vec<_> = (2..=n_pages)
                .map(|page| {
                    let client = client.clone();
                    let url = format!("{base_url}&page={page}");
                    tokio::spawn(async move { fetch_page(&client, &url).await })
                })
                .collect();

            for handle in handles {
                match handle.await {
                    Ok(Ok(patches)) => all.extend(patches),
                    Ok(Err(e)) => eprintln!("Warning: page fetch failed: {e}"),
                    Err(e) => eprintln!("Warning: page task panicked: {e}"),
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
/// Returns map of patch_id → (reviewed_count, acked_count) found in comments.
pub async fn fetch_all_comment_tags(
    client: &Client,
    patch_ids: &[u64],
) -> HashMap<u64, (u32, u32)> {
    let handles: Vec<_> = patch_ids
        .iter()
        .map(|&id| {
            let client = client.clone();
            tokio::spawn(async move {
                let result = fetch_comment_tags_for_patch(&client, id).await;
                (id, result)
            })
        })
        .collect();

    let mut out = HashMap::with_capacity(patch_ids.len());
    for handle in handles {
        match handle.await {
            Ok((id, Ok(counts))) => {
                out.insert(id, counts);
            }
            Ok((id, Err(e))) => eprintln!("Warning: comments fetch failed for patch {id}: {e}"),
            Err(e) => eprintln!("Warning: comments task panicked: {e}"),
        }
    }
    out
}

async fn fetch_comment_tags_for_patch(
    client: &Client,
    patch_id: u64,
) -> anyhow::Result<(u32, u32)> {
    let url = format!("{PATCHWORK_BASE}/patches/{patch_id}/comments/");
    let text = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await?
        .text()
        .await?;

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
/// Returns a map of patch_id → (passed, total).
pub async fn fetch_all_checks(client: &Client, patch_ids: &[u64]) -> HashMap<u64, (u32, u32)> {
    let handles: Vec<_> = patch_ids
        .iter()
        .map(|&id| {
            let client = client.clone();
            tokio::spawn(async move {
                let result = fetch_checks_for_patch(&client, id).await;
                (id, result)
            })
        })
        .collect();

    let mut out = HashMap::with_capacity(patch_ids.len());
    for handle in handles {
        match handle.await {
            Ok((id, Ok(counts))) => {
                out.insert(id, counts);
            }
            Ok((id, Err(e))) => eprintln!("Warning: checks fetch failed for patch {id}: {e}"),
            Err(e) => eprintln!("Warning: checks task panicked: {e}"),
        }
    }
    out
}

async fn fetch_checks_for_patch(client: &Client, patch_id: u64) -> anyhow::Result<(u32, u32)> {
    let url = format!("{PATCHWORK_BASE}/patches/{patch_id}/checks/");
    let text = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await?
        .text()
        .await?;

    let results: Vec<CheckEntry> = if text.trim_start().starts_with('[') {
        serde_json::from_str(&text)?
    } else {
        serde_json::from_str::<ChecksResponse>(&text)?.results
    };

    let total = results.len() as u32;
    let passed = results.iter().filter(|c| c.state == "success").count() as u32;
    Ok((passed, total))
}
