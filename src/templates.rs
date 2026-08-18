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

use std::collections::HashMap;

use minijinja::{context, Environment, Value};

use crate::scoring::ScoredPatch;

const TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="refresh" content="300">
  <title>LTP Dashboard</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

    body {
      background: #1a1a2e;
      color: #e0e0e0;
      font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
      font-size: 14px;
      line-height: 1.5;
      padding: 24px;
    }

    h1 {
      font-size: 1.6rem;
      font-weight: 700;
      color: #ffffff;
      margin-bottom: 4px;
    }

    .subtitle {
      color: #888;
      font-size: 0.85rem;
      margin-bottom: 24px;
    }

    .subtitle a { color: #aaa; text-decoration: none; }
    .subtitle a:hover { color: #fff; text-decoration: underline; }

    /* Tier section */
    .tier-section { margin-bottom: 40px; }

    .tier-header {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 10px;
      padding-bottom: 6px;
      border-bottom: 2px solid #2a2a4a;
    }

    .tier-badge {
      display: inline-block;
      padding: 3px 10px;
      border-radius: 4px;
      font-weight: 700;
      font-size: 0.8rem;
      letter-spacing: 0.05em;
      text-transform: uppercase;
    }

    .tier-P1 .tier-badge { background: #7f1d1d; color: #fca5a5; border: 1px solid #ef4444; }
    .tier-P2 .tier-badge { background: #7c2d12; color: #fdba74; border: 1px solid #f97316; }
    .tier-P3 .tier-badge { background: #713f12; color: #fde68a; border: 1px solid #eab308; }
    .tier-P4 .tier-badge { background: #1e3a5f; color: #93c5fd; border: 1px solid #3b82f6; }
    .tier-P5 .tier-badge { background: #2d2d2d; color: #9ca3af; border: 1px solid #6b7280; }

    .tier-label {
      font-size: 1rem;
      font-weight: 600;
      color: #ccc;
    }

    .tier-count {
      font-size: 0.8rem;
      color: #666;
    }

    /* Table */
    .patch-table {
      width: 100%;
      border-collapse: collapse;
      table-layout: fixed;
    }

    .patch-table th {
      text-align: left;
      padding: 8px 8px;
      background: #16213e;
      color: #7a8fa6;
      font-size: 0.75rem;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      border-bottom: 1px solid #2a2a4a;
      white-space: nowrap;
    }

    .patch-table td {
      padding: 7px 8px;
      border-bottom: 1px solid #1f1f3a;
      vertical-align: top;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .patch-table tr:last-child td { border-bottom: none; }
    .patch-table tr:hover td { background: rgba(255,255,255,0.03); }

    /* Column widths and alignment — .patch-table .col-* wins over .patch-table th (specificity 0,2,0 > 0,1,1) */
    .patch-table .col-num    { text-align: right;  color: #555; font-size: 0.78rem; }
    .patch-table .col-score  { text-align: right;  font-weight: 700; font-size: 0.9rem; }
    .patch-table .col-tier   { text-align: center; }
    .patch-table .col-proj   { text-align: center; }
    .patch-table .col-ver    { text-align: center; color: #aaa; }
    .patch-table .col-age    { text-align: right;  color: #888; white-space: nowrap; }
    .patch-table .col-subm   { text-align: left; color: #aaa; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .patch-table .col-ci     { text-align: center; }
    .ci-pass { color: #34d399; font-weight: 700; font-size: 0.8rem; }
    .ci-fail { color: #f87171; font-weight: 700; font-size: 0.8rem; }
    .ci-warn { color: #fbbf24; font-weight: 700; font-size: 0.8rem; }
    .ci-none { color: #444; font-size: 0.8rem; }

    /* Score colors by tier */
    .tier-P1 .col-score { color: #f87171; }
    .tier-P2 .col-score { color: #fb923c; }
    .tier-P3 .col-score { color: #fbbf24; }
    .tier-P4 .col-score { color: #60a5fa; }
    .tier-P5 .col-score { color: #9ca3af; }

    tr.has-review td { background: rgba(20, 83, 45, 0.18); }
    tr.is-superseded td { background: rgba(127, 29, 29, 0.22); }
    tr:hover td { background: rgba(255,255,255,0.04) !important; }

    /* Inline tier badge in table */
    .badge {
      display: inline-block;
      padding: 1px 6px;
      border-radius: 3px;
      font-size: 0.72rem;
      font-weight: 700;
      letter-spacing: 0.03em;
    }

    .badge-P1 { background: #7f1d1d; color: #fca5a5; }
    .badge-P2 { background: #7c2d12; color: #fdba74; }
    .badge-P3 { background: #713f12; color: #fde68a; }
    .badge-P4 { background: #1e3a5f; color: #93c5fd; }
    .badge-P5 { background: #2d2d2d; color: #9ca3af; }

    /* Subject link */
    .subject-link {
      color: #c4d4e8;
      text-decoration: none;
      word-break: break-word;
      font-size: 1rem;
    }
    .subject-link:hover { color: #ffffff; text-decoration: underline; }
    tr.is-superseded .subject-link { color: #f87171; }
    tr.is-superseded .subject-link:hover { color: #fca5a5; text-decoration: underline; }

    /* Subject inline badges */
    .sbadge {
      display: inline-block;
      font-size: 0.68rem;
      padding: 0 4px;
      border-radius: 2px;
      margin-left: 4px;
      font-weight: 600;
      vertical-align: middle;
      white-space: nowrap;
    }
    .sbadge-stale    { background: #4a1d1d; color: #f87171; }
    .sbadge-rfc      { background: #431407; color: #fb923c; }
    .sbadge-series   { background: #1e3a5f; color: #93c5fd; }
    .sbadge-reviewed { background: #14532d; color: #86efac; }
    .sbadge-acked    { background: #134e4a; color: #5eead4; }
    .sbadge-review   { background: #312e81; color: #a5b4fc; }
    .sbadge-superseded { background: #7f1d1d; color: #fca5a5; }
    .sbadge-smalldiff  { background: #064e3b; color: #6ee7b7; }
    .sbadge-largediff  { background: #854d0e; color: #fef9c3; }
    .sbadge-project    { background: #2d3748; color: #e2e8f0; }

    /* Series rows */
    .series-toggle { cursor: pointer; }
    .series-toggle:hover td { background: rgba(255,255,255,0.06) !important; }
    .series-indicator { display: inline-block; width: 14px; font-size: 0.65rem; color: #888; }
    .series-indent { display: inline-block; width: 14px; }
    .series-detail td { opacity: 0.85; }
    .series-detail.has-review td { background: rgba(20, 83, 45, 0.18); }
    .series-detail td:first-child { border-left: 3px solid #2a2a4a; }

    /* Empty state */
    .empty-row td {
      text-align: center;
      color: #444;
      padding: 16px;
      font-style: italic;
    }

    /* Summary */
    .summary {
      margin-top: 40px;
      padding: 20px 24px;
      background: #16213e;
      border: 1px solid #2a2a4a;
      border-radius: 8px;
      max-width: 480px;
    }

    .summary h2 {
      font-size: 0.9rem;
      font-weight: 600;
      color: #7a8fa6;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      margin-bottom: 14px;
    }

    .summary-grid {
      display: grid;
      grid-template-columns: 1fr auto;
      gap: 6px 24px;
    }

    .summary-label { color: #aaa; }
    .summary-value { font-weight: 700; color: #fff; text-align: right; }
    .summary-value.warn { color: #f87171; }

    .refresh-note {
      margin-top: 16px;
      font-size: 0.75rem;
      color: #555;
    }

    /* Reasons tooltip area */
    .reasons {
      font-size: 0.72rem;
      color: #555;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
  </style>
</head>
<body>

<h1>LTP Dashboard</h1>
<p class="subtitle">
  Fetched {{ total }} patches from projects {{ projects }}
  &mdash; generated {{ generated_at }}
</p>

{% for tier in tiers %}
<div class="tier-section">
  <div class="tier-header tier-{{ tier.tier_id }}">
    <span class="tier-badge">{{ tier.tier_id }}</span>
    <span class="tier-label">{{ tier.tier_label }}</span>
    <span class="tier-count">({{ tier.patches | length }} patches)</span>
  </div>

  <table class="patch-table">
    <colgroup>
      <col style="width:48px">
      <col style="width:68px">
      <col style="width:76px">
      <col style="width:76px">
      <col style="width:52px">
      <col style="width:64px">
      <col style="width:120px">
      {% if show_checks %}<col style="width:80px">{% endif %}
      <col>
    </colgroup>
    <thead>
      <tr>
        <th class="col-num">#</th>
        <th class="col-score">Score</th>
        <th class="col-tier">Tier</th>
        <th class="col-proj">Project</th>
        <th class="col-ver">Ver</th>
        <th class="col-age">Age</th>
        <th class="col-subm">Submitter</th>
        {% if show_checks %}<th class="col-ci">CI</th>{% endif %}
        <th>Subject</th>
      </tr>
    </thead>
    <tbody>
      {% if tier.patches | length == 0 %}
      <tr class="empty-row">
        <td colspan="{{ 8 + (show_checks | int) }}">No patches in this tier</td>
      </tr>
      {% else %}
      {% for p in tier.patches %}
      {% if p.is_series %}
      <tr class="tier-{{ p.tier }} series-toggle{% if p.reviewed > 0 or p.acked > 0 %} has-review{% endif %}{% if p.superseded %} is-superseded{% endif %}" onclick="toggleSeries(this, '{{ p.series_id }}')">
        <td class="col-num">{{ p.num }}</td>
        <td class="col-score">{{ p.score }}</td>
        <td class="col-tier"><span class="badge badge-{{ p.tier }}">{{ p.tier }}</span></td>
        <td class="col-proj"><span class="sbadge sbadge-project">{{ p.project }}</span></td>
        <td class="col-ver">v{{ p.version }}</td>
        <td class="col-age">{{ p.days }}d</td>
        <td class="col-subm" title="{{ p.submitter }}">{{ p.submitter }}</td>
        {% if show_checks %}
        <td class="col-ci">
          {% if p.checks_total == 0 %}<span class="ci-none">—</span>
          {% elif p.checks_passed == p.checks_total %}<span class="ci-pass">{{ p.checks_passed }}/{{ p.checks_total }}</span>
          {% elif p.checks_failed > 0 %}<span class="ci-fail">{{ p.checks_passed }}/{{ p.checks_total }}</span>
          {% else %}<span class="ci-warn">{{ p.checks_passed }}/{{ p.checks_total }}</span>
          {% endif %}
        </td>
        {% endif %}
        <td>
          <span class="series-indicator" id="ind-{{ p.series_id }}">&#9654;</span>
          <a class="subject-link" href="{{ p.url }}" target="_blank" rel="noopener" onclick="event.stopPropagation()">{{ p.name }}</a>
          <span class="sbadge sbadge-series">SERIES {{ p.series_size }}</span>
          {% if p.days > 60 %}<span class="sbadge sbadge-stale">Stale</span>{% endif %}
          {% if p.rfc %}<span class="sbadge sbadge-rfc">RFC</span>{% endif %}
          {% if p.state == "under-review" %}<span class="sbadge sbadge-review">Under Review</span>{% endif %}
          {% if p.reviewed > 0 %}<span class="sbadge sbadge-reviewed">Reviewed-by</span>{% endif %}
          {% if p.acked > 0 %}<span class="sbadge sbadge-acked">Acked-by</span>{% endif %}
          {% if p.superseded %}<span class="sbadge sbadge-superseded">Superseded</span>{% endif %}
          <div class="reasons">{{ p.reasons }}</div>
        </td>
      </tr>
      {% for c in p.patches %}
      <tr class="tier-{{ c.tier }} series-detail series-{{ p.series_id }}{% if c.reviewed > 0 or c.acked > 0 %} has-review{% endif %}{% if c.superseded %} is-superseded{% endif %}" style="display:none">
        <td class="col-num"></td>
        <td class="col-score">{{ c.score }}</td>
        <td class="col-tier"><span class="badge badge-{{ c.tier }}">{{ c.tier }}</span></td>
        <td class="col-proj"><span class="sbadge sbadge-project">{{ c.project }}</span></td>
        <td class="col-ver">v{{ c.version }}</td>
        <td class="col-age">{{ c.days }}d</td>
        <td class="col-subm" title="{{ c.submitter }}">{{ c.submitter }}</td>
        {% if show_checks %}
        <td class="col-ci">
          {% if c.checks_total == 0 %}<span class="ci-none">—</span>
          {% elif c.checks_passed == c.checks_total %}<span class="ci-pass">{{ c.checks_passed }}/{{ c.checks_total }}</span>
          {% elif c.checks_failed > 0 %}<span class="ci-fail">{{ c.checks_passed }}/{{ c.checks_total }}</span>
          {% else %}<span class="ci-warn">{{ c.checks_passed }}/{{ c.checks_total }}</span>
          {% endif %}
        </td>
        {% endif %}
        <td>
          <span class="series-indent"></span>
          <a class="subject-link" href="{{ c.url }}" target="_blank" rel="noopener">{{ c.name }}</a>
          {% if c.days > 60 %}<span class="sbadge sbadge-stale">Stale</span>{% endif %}
          {% if c.rfc %}<span class="sbadge sbadge-rfc">RFC</span>{% endif %}
          {% if c.state == "under-review" %}<span class="sbadge sbadge-review">Under Review</span>{% endif %}
          {% if c.reviewed > 0 %}<span class="sbadge sbadge-reviewed">Reviewed-by</span>{% endif %}
          {% if c.acked > 0 %}<span class="sbadge sbadge-acked">Acked-by</span>{% endif %}
          {% if c.superseded %}<span class="sbadge sbadge-superseded">Superseded</span>{% endif %}
          {% if c.diff_lines > 200 %}<span class="sbadge sbadge-largediff">{{ c.diff_lines }} lines</span>
          {% elif c.diff_lines > 0 and c.diff_lines <= 50 %}<span class="sbadge sbadge-smalldiff">{{ c.diff_lines }} lines</span>{% endif %}
          <div class="reasons">{{ c.reasons }}</div>
        </td>
      </tr>
      {% endfor %}
      {% else %}
      <tr class="tier-{{ p.tier }}{% if p.reviewed > 0 or p.acked > 0 %} has-review{% endif %}{% if p.superseded %} is-superseded{% endif %}">
        <td class="col-num">{{ p.num }}</td>
        <td class="col-score">{{ p.score }}</td>
        <td class="col-tier"><span class="badge badge-{{ p.tier }}">{{ p.tier }}</span></td>
        <td class="col-proj"><span class="sbadge sbadge-project">{{ p.project }}</span></td>
        <td class="col-ver">v{{ p.version }}</td>
        <td class="col-age">{{ p.days }}d</td>
        <td class="col-subm" title="{{ p.submitter }}">{{ p.submitter }}</td>
        {% if show_checks %}
        <td class="col-ci">
          {% if p.checks_total == 0 %}<span class="ci-none">—</span>
          {% elif p.checks_passed == p.checks_total %}<span class="ci-pass">{{ p.checks_passed }}/{{ p.checks_total }}</span>
          {% elif p.checks_failed > 0 %}<span class="ci-fail">{{ p.checks_passed }}/{{ p.checks_total }}</span>
          {% else %}<span class="ci-warn">{{ p.checks_passed }}/{{ p.checks_total }}</span>
          {% endif %}
        </td>
        {% endif %}
        <td>
          <a class="subject-link" href="{{ p.url }}" target="_blank" rel="noopener">{{ p.name }}</a>
          {% if p.days > 60 %}<span class="sbadge sbadge-stale">Stale</span>{% endif %}
          {% if p.rfc %}<span class="sbadge sbadge-rfc">RFC</span>{% endif %}
          {% if p.series_size > 5 %}<span class="sbadge sbadge-series">SERIES {{ p.series_size }}</span>{% endif %}
          {% if p.state == "under-review" %}<span class="sbadge sbadge-review">Under Review</span>{% endif %}
          {% if p.reviewed > 0 %}<span class="sbadge sbadge-reviewed">Reviewed-by</span>{% endif %}
          {% if p.acked > 0 %}<span class="sbadge sbadge-acked">Acked-by</span>{% endif %}
          {% if p.superseded %}<span class="sbadge sbadge-superseded">Superseded</span>{% endif %}
          {% if p.diff_lines > 200 %}<span class="sbadge sbadge-largediff">{{ p.diff_lines }} lines</span>
          {% elif p.diff_lines > 0 and p.diff_lines <= 50 %}<span class="sbadge sbadge-smalldiff">{{ p.diff_lines }} lines</span>{% endif %}
          <div class="reasons">{{ p.reasons }}</div>
        </td>
      </tr>
      {% endif %}
      {% endfor %}
      {% endif %}
    </tbody>
  </table>
</div>
{% endfor %}

<div class="summary">
  <h2>Summary</h2>
  <div class="summary-grid">
    <span class="summary-label">Total patches evaluated</span>
    <span class="summary-value">{{ total }}</span>
    <span class="summary-label">Patches &gt; 60 days old (stale)</span>
    <span class="summary-value {% if stale_count > 0 %}warn{% endif %}">{{ stale_count }}</span>
    <span class="summary-label">RFC patches</span>
    <span class="summary-value">{{ rfc_count }}</span>
    <span class="summary-label">With Reviewed-by (ready)</span>
    <span class="summary-value">{{ reviewed_count }}</span>
    <span class="summary-label">Superseded patches</span>
    <span class="summary-value {% if superseded_count > 0 %}warn{% endif %}">{{ superseded_count }}</span>
  </div>
  <p class="refresh-note">Page auto-refreshes every 5 minutes.</p>
</div>

<script>
function toggleSeries(row, seriesId) {
  var details = document.querySelectorAll('.series-' + seriesId);
  var ind = document.getElementById('ind-' + seriesId);
  var visible = details.length > 0 && details[0].style.display !== 'none';
  for (var i = 0; i < details.length; i++) {
    details[i].style.display = visible ? 'none' : '';
  }
  if (ind) ind.innerHTML = visible ? '&#9654;' : '&#9660;';
}
</script>

</body>
</html>
"#;

pub struct TemplateData<'a> {
    pub patches: &'a [ScoredPatch],
    pub generated_at: String,
    pub show_checks: bool,
    pub projects: String,
}

fn patch_context(p: &ScoredPatch, num: usize) -> Value {
    context! {
        num,
        project => &p.label,
        score => p.score,
        tier => p.tier,
        tier_label => p.tier_label,
        version => p.version,
        days => p.days,
        state => &p.state,
        rfc => p.rfc,
        series_size => p.series_size,
        reviewed => p.reviewed,
        acked => p.acked,
        superseded => p.superseded,
        diff_lines => p.diff_lines,
        name => &p.name,
        submitter => &p.submitter,
        url => &p.url,
        reasons => p.reasons.join(" · "),
        checks_passed => p.checks_passed,
        checks_failed => p.checks_failed,
        checks_total => p.checks_total,
    }
}

pub fn render_index(data: &TemplateData<'_>) -> Result<String, minijinja::Error> {
    let mut env = Environment::new();
    env.add_template("index.html", TEMPLATE)?;
    let tmpl = env.get_template("index.html")?;

    let total = data.patches.len();
    let stale_count = data.patches.iter().filter(|p| p.days > 60).count();
    let rfc_count = data.patches.iter().filter(|p| p.rfc).count();
    let reviewed_count = data.patches.iter().filter(|p| p.reviewed >= 1).count();
    let superseded_count = data.patches.iter().filter(|p| p.superseded).count();

    // Build per-tier groups in canonical order
    let tier_order: &[(&str, &str)] = &[
        ("P1", "URGENT"),
        ("P2", "HIGH"),
        ("P3", "NORMAL"),
        ("P4", "LOW"),
        ("P5", "DEFER"),
    ];

    // Pre-group multi-patch series across all tiers. Each series is assigned
    // to the tier of its best-scoring patch so the whole group stays together.
    let mut series_map: HashMap<u64, Vec<&ScoredPatch>> = HashMap::new();
    let mut in_series: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for p in data.patches.iter() {
        if p.series_size > 1 {
            if let Some(sid) = p.series_id {
                series_map.entry(sid).or_default().push(p);
            }
        }
    }
    // Only keep groups with >1 member; mark their patch ids
    series_map.retain(|_, members| members.len() > 1);
    for members in series_map.values() {
        for p in members {
            in_series.insert(p.id);
        }
    }
    // Map series_id → tier of best patch
    let series_tier: HashMap<u64, &str> = series_map
        .iter()
        .map(|(&sid, members)| {
            let best = members.iter().max_by_key(|p| p.score).unwrap();
            (sid, best.tier)
        })
        .collect();

    let mut counter = 1usize;
    let tiers: Vec<Value> = tier_order
        .iter()
        .map(|(tid, tlabel)| {
            // Standalone patches in this tier + series assigned to this tier
            let mut items: Vec<Value> = Vec::new();

            // Collect standalone patches and series for this tier, preserving
            // score-descending order from the input.
            let mut seen_series: std::collections::HashSet<u64> = std::collections::HashSet::new();
            for p in data.patches.iter() {
                if in_series.contains(&p.id) {
                    let sid = p.series_id.unwrap();
                    if series_tier.get(&sid) != Some(tid) {
                        continue;
                    }
                    if !seen_series.insert(sid) {
                        continue;
                    }
                    let members = &series_map[&sid];
                    let best = members.iter().max_by_key(|p| p.score).unwrap();
                    let num = counter;
                    counter += 1;
                    let child_patches: Vec<Value> =
                        members.iter().map(|p| patch_context(p, counter)).collect();
                    items.push(context! {
                        is_series => true,
                        series_id => sid,
                        num,
                        project => &best.label,
                        score => best.score,
                        tier => best.tier,
                        tier_label => best.tier_label,
                        version => best.version,
                        days => best.days,
                        series_size => members.len(),
                        name => &best.name,
                        url => &best.url,
                        submitter => &best.submitter,
                        reviewed => members.iter().map(|p| p.reviewed).sum::<u32>(),
                        acked => members.iter().map(|p| p.acked).sum::<u32>(),
                        superseded => best.superseded,
                        rfc => best.rfc,
                        diff_lines => members.iter().map(|p| p.diff_lines).sum::<u32>(),
                        state => &best.state,
                        reasons => best.reasons.join(" · "),
                        checks_passed => best.checks_passed,
                        checks_failed => best.checks_failed,
                        checks_total => best.checks_total,
                        patches => child_patches,
                    });
                } else if p.tier == *tid {
                    let num = counter;
                    counter += 1;
                    items.push(context! {
                        is_series => false,
                        num,
                        project => &p.label,
                        score => p.score,
                        tier => p.tier,
                        tier_label => p.tier_label,
                        version => p.version,
                        days => p.days,
                        state => &p.state,
                        rfc => p.rfc,
                        series_size => p.series_size,
                        reviewed => p.reviewed,
                        acked => p.acked,
                        superseded => p.superseded,
                        diff_lines => p.diff_lines,
                        name => &p.name,
                        url => &p.url,
                        reasons => p.reasons.join(" · "),
                        checks_passed => p.checks_passed,
                        checks_failed => p.checks_failed,
                        checks_total => p.checks_total,
                    });
                }
            }

            context! {
                tier_id => *tid,
                tier_label => *tlabel,
                patches => items,
            }
        })
        .collect();

    let html = tmpl.render(context! {
        total,
        stale_count,
        rfc_count,
        reviewed_count,
        superseded_count,
        generated_at => &data.generated_at,
        show_checks => data.show_checks,
        projects => &data.projects,
        tiers,
    })?;

    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_superseded() {
        let patches = vec![ScoredPatch {
            project: "ltp".to_string(),
            label: "LTP".to_string(),
            id: 1,
            name: "[PATCH] syscalls/read01: fix bug".to_string(),
            submitter: "Alice".to_string(),
            date: "2026-01-01T00:00:00Z".to_string(),
            days: 10,
            state: "new".to_string(),
            version: 1,
            rfc: false,
            series_id: None,
            series_size: 1,
            reviewed: 0,
            acked: 0,
            delegated: false,
            fix_keyword: true,
            new_test: false,
            sob_count: 1,
            lib_pts: 0,
            diff_lines: 10,
            checks_passed: 0,
            checks_failed: 0,
            checks_total: 0,
            score: 50,
            reasons: vec![],
            url: "http://example.com/1".to_string(),
            tier: "P3",
            tier_label: "NORMAL",
            superseded: true,
        }];

        let data = TemplateData {
            patches: &patches,
            generated_at: "2026-01-01 00:00:00 UTC".to_string(),
            show_checks: false,
            projects: "LTP".to_string(),
        };

        let html = render_index(&data).unwrap();
        assert!(html.contains("is-superseded"));
        assert!(html.contains("sbadge-superseded"));
        assert!(html.contains("Superseded"));
    }
}
