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

use chrono::{DateTime, Local, Utc};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::patchwork::RawPatch;

// ---------------------------------------------------------------------------
// Compiled regexes (lazy, thread-safe)
// ---------------------------------------------------------------------------

fn version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\[(?:[^\]]*?[,\s])?v(\d+)").unwrap())
}

fn rfc_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\[(?:[^\]]*,)?RFC(?:[,\]])").unwrap())
}

fn cover_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\[(?:[^\]]*[,\s])?0/\d+").unwrap())
}

fn series_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\[(?:[^\]]*[,\s])?(\d+)/(\d+)").unwrap())
}

fn fix_keywords_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(fix(es)?|bug|regression|broken|bugfix|revert)\b").unwrap()
    })
}

fn new_test_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(rewrite|new\s+test|add\s+test)\b").unwrap())
}

fn lib_subject_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(tst_|safe_|lapi|^lib:|configure\.ac)\b").unwrap())
}

fn include_subject_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(lapi|include/|syscalls\.h)\b").unwrap())
}

fn subject_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*\[.*?\]\s*").unwrap())
}

// ---------------------------------------------------------------------------
// Individual scoring functions
// ---------------------------------------------------------------------------

pub fn score_version(version: u32) -> i32 {
    match version {
        1 => 0,
        2 => 15,
        3 => 25,
        4 => 35,
        _ => 40,
    }
}

/// Inverted-U curve: peaks at 31-60 days (genuinely neglected),
/// then decays for very old patches (likely stale).
pub fn score_age(days: i64) -> i32 {
    if days < 7 {
        0
    } else if days < 15 {
        10
    } else if days < 31 {
        20
    } else if days < 61 {
        45
    } else if days < 91 {
        35
    } else if days < 181 {
        20
    } else if days < 366 {
        10
    } else {
        5
    }
}

pub fn score_series(size: u32) -> i32 {
    if size == 1 {
        10
    } else if size <= 5 {
        0
    } else if size <= 10 {
        -5
    } else {
        -15
    }
}

pub fn score_sob(count: u32) -> i32 {
    match count {
        0 | 1 => 0,
        2 => 5,
        3 => 10,
        _ => 15,
    }
}

pub fn parse_version(name: &str) -> u32 {
    version_re()
        .captures(name)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(1)
}

pub fn parse_series_size(name: &str, series: &[crate::patchwork::SeriesRef]) -> u32 {
    // Try the series field first
    for s in series {
        if let Some(t) = s.total.or(s.count) {
            if t > 0 {
                return t;
            }
        }
    }
    // Fall back to parsing the subject
    series_re()
        .captures(name)
        .and_then(|c| c.get(2))
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(1)
}

pub fn age_days(date_str: &str) -> i64 {
    // Patchwork returns dates with or without a timezone suffix (e.g. "Z" or "+00:00").
    // Append "+00:00" if no timezone designator is present so RFC3339 parsing succeeds.
    let fixed = if date_str.ends_with('Z')
        || date_str.contains('+')
        || date_str.contains('-') && date_str.len() > 19
    {
        date_str.replace('Z', "+00:00")
    } else {
        format!("{date_str}+00:00")
    };
    match DateTime::parse_from_rfc3339(&fixed) {
        Ok(dt) => {
            let today = Local::now().date_naive();
            let patch_date = dt.with_timezone(&Utc).date_naive();
            (today - patch_date).num_days().max(0)
        }
        Err(_) => 30,
    }
}

pub fn lib_score_from_subject(name: &str) -> i32 {
    if lib_subject_re().is_match(name) {
        15
    } else if include_subject_re().is_match(name) {
        10
    } else {
        0
    }
}

pub fn is_cover(name: &str) -> bool {
    cover_re().is_match(name)
}

// ---------------------------------------------------------------------------
// Scored patch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScoredPatch {
    pub id: u64,
    pub name: String,
    pub submitter: String,
    pub date: String,
    pub days: i64,
    pub state: String,
    pub version: u32,
    pub rfc: bool,
    pub series_size: u32,
    pub reviewed: u32,
    pub acked: u32,
    pub delegated: bool,
    pub fix_keyword: bool,
    pub new_test: bool,
    pub sob_count: u32,
    pub lib_pts: i32,
    pub checks_passed: u32,
    pub checks_total: u32,
    pub score: i32,
    pub reasons: Vec<String>,
    pub url: String,
    pub tier: &'static str,
    pub tier_label: &'static str,
    pub superseded: bool,
}

impl ScoredPatch {
    pub fn notes(&self) -> String {
        let mut parts = Vec::new();
        if self.superseded {
            parts.push("SUPERSEDED".to_string());
        }
        if self.reviewed > 0 {
            parts.push("Reviewed-by".to_string());
        }
        if self.acked > 0 {
            parts.push("Acked-by".to_string());
        }
        if self.rfc {
            parts.push("RFC".to_string());
        }
        if self.lib_pts >= 15 {
            parts.push("lib/".to_string());
        } else if self.lib_pts >= 10 {
            parts.push("include/".to_string());
        } else if self.lib_pts > 0 {
            parts.push("tools/".to_string());
        }
        if self.sob_count > 1 {
            parts.push(format!("SOB:{}", self.sob_count));
        }
        if self.days > 60 {
            parts.push("STALE".to_string());
        }
        if self.series_size > 5 {
            parts.push(format!("SERIES {}", self.series_size));
        }
        if parts.is_empty() {
            "—".to_string()
        } else {
            parts.join(", ")
        }
    }
}

// ---------------------------------------------------------------------------
// Tier classification
// ---------------------------------------------------------------------------

pub fn classify(score: i32) -> (&'static str, &'static str) {
    if score >= 80 {
        ("P1", "URGENT")
    } else if score >= 60 {
        ("P2", "HIGH")
    } else if score >= 40 {
        ("P3", "NORMAL")
    } else if score >= 20 {
        ("P4", "LOW")
    } else {
        ("P5", "DEFER")
    }
}

// ---------------------------------------------------------------------------
// Main scoring entry point
// ---------------------------------------------------------------------------

pub fn score_patch(patch: &RawPatch) -> ScoredPatch {
    let name = &patch.name;
    let state = &patch.state;

    let version = parse_version(name);
    let rfc = rfc_re().is_match(name);
    let series_size = parse_series_size(name, &patch.series);
    let days = age_days(&patch.date);
    let fix_keyword = fix_keywords_re().is_match(name);
    let new_test = new_test_re().is_match(name);
    let lib_pts = lib_score_from_subject(name);
    let reviewed = patch.tags.reviewed_by_count;
    let acked = patch.tags.acked_by_count;
    let sob_count = patch.tags.signed_off_by_count;
    let delegated = patch.delegate.is_some();

    let mut score: i32 = 0;
    let mut reasons: Vec<String> = Vec::new();

    // Rule 1 – version
    let v = score_version(version);
    if v != 0 {
        score += v;
        reasons.push(format!("v{version}(+{v})"));
    }

    // Rule 2 – fix keywords
    if fix_keyword {
        score += 45;
        reasons.push("fix(+45)".to_string());
    }

    // Rule 2b – new/rewrite/add test
    if new_test {
        score += 15;
        reasons.push("new-test(+15)".to_string());
    }

    // Rule 3 – age
    let a = score_age(days);
    if a != 0 {
        score += a;
        reasons.push(format!("age:{days}d(+{a})"));
    }

    // Rule 4 – RFC
    if rfc {
        score -= 25;
        reasons.push("RFC(-25)".to_string());
    }

    // Rule 5 – lib/infra
    if lib_pts != 0 {
        score += lib_pts;
        reasons.push(format!("lib(+{lib_pts})"));
    }

    // Rule 6 – series size
    let s = score_series(series_size);
    if s != 0 {
        score += s;
        reasons.push(format!("series:{series_size}({s:+})"));
    }

    // Rule 7 – review tags
    if reviewed >= 1 {
        score += 20;
        reasons.push("Reviewed-by(+20)".to_string());
    }
    if acked >= 1 {
        score += 10;
        reasons.push("Acked-by(+10)".to_string());
    }

    // Rule 7b – SOB count
    let sob_pts = score_sob(sob_count);
    if sob_pts != 0 {
        score += sob_pts;
        reasons.push(format!("SOB:{sob_count}(+{sob_pts})"));
    }

    // Rule 8 – delegated
    if delegated {
        score -= 10;
        reasons.push("delegated(-10)".to_string());
    }

    // Rule 9 – state
    if state == "under-review" {
        score -= 20;
        reasons.push("under-review(-20)".to_string());
    }

    let (tier, tier_label) = classify(score);

    let url = patch.web_url.clone().unwrap_or_else(|| {
        format!(
            "https://patchwork.ozlabs.org/project/ltp/patch/{}/",
            patch.id
        )
    });

    ScoredPatch {
        id: patch.id,
        name: name.clone(),
        submitter: patch.submitter.name.clone().unwrap_or_default(),
        date: patch.date.clone(),
        days,
        state: state.clone(),
        version,
        rfc,
        series_size,
        reviewed,
        acked,
        delegated,
        fix_keyword,
        new_test,
        sob_count,
        lib_pts,
        checks_passed: 0,
        checks_total: 0,
        score,
        reasons,
        url,
        tier,
        tier_label,
        superseded: false,
    }
}

// ---------------------------------------------------------------------------
// Superseded detection
// ---------------------------------------------------------------------------

/// Strip the leading `[PATCH ...]` tag from a subject to get the bare title.
pub fn base_subject(name: &str) -> String {
    subject_tag_re().replace(name, "").trim().to_lowercase()
}

/// Mark patches that have a newer version from the same submitter as superseded.
pub fn mark_superseded(patches: &mut [ScoredPatch]) {
    // First pass: find the max version for each (submitter, base_subject) group.
    let mut max_version: HashMap<(String, String), u32> = HashMap::new();
    for p in patches.iter() {
        let key = (p.submitter.clone(), base_subject(&p.name));
        let entry = max_version.entry(key).or_insert(0);
        if p.version > *entry {
            *entry = p.version;
        }
    }

    // Second pass: flag patches whose version is below the group max.
    for p in patches.iter_mut() {
        let key = (p.submitter.clone(), base_subject(&p.name));
        if let Some(&max_v) = max_version.get(&key) {
            if max_v > 1 && p.version < max_v {
                p.superseded = true;
            }
        }
    }
}
