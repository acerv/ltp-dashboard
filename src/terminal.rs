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

use chrono::Local;

use crate::scoring::ScoredPatch;

// ANSI codes
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[91m";
const ORANGE: &str = "\x1b[38;5;208m";
const YELLOW: &str = "\x1b[93m";
const BLUE: &str = "\x1b[94m";
const WHITE: &str = "\x1b[97m";
const GRAY: &str = "\x1b[90m";

fn tier_color(tier: &str) -> &'static str {
    match tier {
        "P1" => RED,
        "P2" => ORANGE,
        "P3" => YELLOW,
        "P4" => BLUE,
        _ => WHITE,
    }
}

fn tier_label(tier: &str) -> &'static str {
    match tier {
        "P1" => "🔴 URGENT",
        "P2" => "🟠 HIGH",
        "P3" => "🟡 NORMAL",
        "P4" => "🔵 LOW",
        _ => "⚪ DEFER",
    }
}

pub fn print_queue(
    patches: &[ScoredPatch],
    counts: &std::collections::HashMap<String, usize>,
    show_checks: bool,
) {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let total = patches.len();
    let new_count = counts.get("new").copied().unwrap_or(0);
    let under_count = counts.get("under-review").copied().unwrap_or(0);
    let needs_count = counts.get("needs-review-ack").copied().unwrap_or(0);
    let stale_count = patches.iter().filter(|p| p.days > 60).count();
    let rfc_count = patches.iter().filter(|p| p.rfc).count();
    let waiting_count = patches.iter().filter(|p| p.reviewed >= 1).count();
    let superseded_count = patches.iter().filter(|p| p.superseded).count();

    let rule = "─".repeat(80);

    println!("\n{BOLD}LTP Patch Review Queue — {today}{RESET}\n");
    println!(
        "Fetched {BOLD}{total}{RESET} patches  \
         (new: {new_count}  under-review: {under_count}  needs-review: {needs_count})\n"
    );
    println!("{rule}");

    const W_NUM: usize = 3;
    const W_SCORE: usize = 5;
    const W_VER: usize = 3;
    const W_AGE: usize = 5;
    const W_CI: usize = 5;
    let w_subj: usize = if show_checks { 52 } else { 58 };

    let tier_order = ["P1", "P2", "P3", "P4", "P5"];

    let mut counter = 1usize;
    for tid in tier_order {
        let color = tier_color(tid);
        let label = tier_label(tid);
        let group: Vec<&ScoredPatch> = patches.iter().filter(|p| p.tier == tid).collect();

        println!(
            "\n{color}{BOLD}{label}{RESET}  {GRAY}({}){RESET}\n",
            group.len()
        );

        if group.is_empty() {
            println!("  {DIM}None{RESET}");
            continue;
        }

        // Header
        let ci_hdr = if show_checks {
            format!("  {:>W_CI$}", "CI")
        } else {
            String::new()
        };
        let ci_sep = if show_checks {
            format!("  {}", "─".repeat(W_CI))
        } else {
            String::new()
        };
        println!(
            "{BOLD}  {num:>W_NUM$}  {score:>W_SCORE$}  {ver:>W_VER$}  {age:>W_AGE$}{ci}  {subj:<w_subj$}  Notes{RESET}",
            num = "#", score = "Score", ver = "Ver", age = "Age", ci = ci_hdr, subj = "Subject",
        );
        println!(
            "{GRAY}  {s1}  {s2}  {s3}  {s4}{s5}  {s6}  {s7}{RESET}",
            s1 = "─".repeat(W_NUM),
            s2 = "─".repeat(W_SCORE),
            s3 = "─".repeat(W_VER),
            s4 = "─".repeat(W_AGE),
            s5 = ci_sep,
            s6 = "─".repeat(w_subj),
            s7 = "─".repeat(22),
        );

        for p in group {
            let score_str = format!("{color}{BOLD}{:>W_SCORE$}{RESET}", p.score);
            let ver_str = format!("v{}", p.version);
            let age_str = format!("{}d", p.days);
            let ci_str = if show_checks {
                let s = if p.checks_total == 0 {
                    format!("{GRAY}{:>W_CI$}{RESET}", "—")
                } else if p.checks_passed == p.checks_total {
                    format!(
                        "{YELLOW}{:>W_CI$}{RESET}",
                        format!("{}/{}", p.checks_passed, p.checks_total)
                    )
                } else if p.checks_failed > 0 {
                    format!(
                        "{RED}{:>W_CI$}{RESET}",
                        format!("{}/{}", p.checks_passed, p.checks_total)
                    )
                } else {
                    format!(
                        "{YELLOW}{:>W_CI$}{RESET}",
                        format!("{}/{}", p.checks_passed, p.checks_total)
                    )
                };
                format!("  {s}")
            } else {
                String::new()
            };
            let name_trunc = if p.name.len() > w_subj {
                format!("{}…", &p.name[..w_subj.saturating_sub(1)])
            } else {
                p.name.clone()
            };
            let link = format!("\x1b]8;;{}\x1b\\{name_trunc}\x1b]8;;\x1b\\", p.url);
            let vis_len = name_trunc.len();
            let pad_len = w_subj.saturating_sub(vis_len);
            let subj_pad = format!("{link}{}", " ".repeat(pad_len));
            let notes = p.notes();

            println!(
                "  {num:>W_NUM$}  {score}  {ver:>W_VER$}  {age:>W_AGE$}{ci}  {subj}  {GRAY}{notes}{RESET}",
                num = counter, score = score_str, ver = ver_str, age = age_str,
                ci = ci_str, subj = subj_pad,
            );
            counter += 1;
        }
    }

    println!("\n{rule}\n");
    println!("{BOLD}Summary{RESET}\n");
    let stale_note = if stale_count > 0 {
        format!("  {RED}← needs attention{RESET}")
    } else {
        String::new()
    };
    println!("  Total patches evaluated:           {BOLD}{total}{RESET}");
    println!("  Patches > 60 days old:             {BOLD}{stale_count}{RESET}{stale_note}");
    println!("  RFC patches:                       {BOLD}{rfc_count}{RESET}");
    println!("  Patches with Reviewed-by (ready):  {BOLD}{waiting_count}{RESET}");
    if superseded_count > 0 {
        println!("  Superseded patches:                {BOLD}{superseded_count}{RESET}  {RED}← older versions{RESET}");
    } else {
        println!("  Superseded patches:                {BOLD}{superseded_count}{RESET}");
    }
    println!();
}
