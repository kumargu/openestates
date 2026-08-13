use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use backend::knowledge::FactValue;
use backend::serving::{
    read_edges_parquet, read_entities_parquet, read_facts_parquet, ServingEdgeRecord,
    ServingEntityRecord, ServingFactRecord,
};
use serde::Serialize;
use serde_json::Value;

const DEFAULT_TARGET: &str = "prestige waterford";
const DEFAULT_OUTPUT: &str = "tmp/fact_graph/prestige-waterford.html";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = CliOptions::parse()?;
    let project_root = options.project_root.unwrap_or_else(default_project_root);
    let bundle_version = match options.version {
        Some(version) => version,
        None => current_serving_bundle_version(&project_root)?,
    };
    let bundle_dir = project_root
        .join("data/lake/serving/search_bundle")
        .join(format!("version={bundle_version}"));

    let entities =
        read_entities_parquet(&fs::read(bundle_dir.join("entities/part-00000.parquet"))?)?;
    let facts = read_facts_parquet(&fs::read(bundle_dir.join("facts/part-00000.parquet"))?)?;
    let edges = read_edges_parquet(&fs::read(bundle_dir.join("edges/part-00000.parquet"))?)?;

    let target = resolve_target(&entities, &options.target)?;
    let graph = build_graph(
        &bundle_version,
        target,
        &entities,
        &facts,
        &edges,
        options.depth,
        options.max_facts_per_entity,
    );
    let html = render_html(&graph)?;

    let output = project_root.join(options.output);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, html)?;
    println!("{}", output.display());
    Ok(())
}

fn default_project_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.file_name().is_some_and(|name| name == "backend") {
        cwd.parent().unwrap_or(&cwd).to_path_buf()
    } else {
        cwd
    }
}

fn current_serving_bundle_version(
    project_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let current_path = project_root
        .join("data/lake/manifests/assets/search_serving_bundle/partition=global/current.json");
    let current: Value = serde_json::from_slice(&fs::read(current_path)?)?;
    current
        .get("version")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "current serving-bundle pointer is missing version".into())
}

fn resolve_target<'a>(
    entities: &'a [ServingEntityRecord],
    target: &str,
) -> Result<&'a ServingEntityRecord, Box<dyn std::error::Error>> {
    let normalized = target.to_ascii_lowercase();
    entities
        .iter()
        .find(|entity| entity.entity_id == target)
        .or_else(|| {
            entities.iter().find(|entity| {
                entity.entity_type == "society" && entity.name.to_ascii_lowercase() == normalized
            })
        })
        .or_else(|| {
            entities.iter().find(|entity| {
                entity.entity_type == "society"
                    && entity.name.to_ascii_lowercase().contains(&normalized)
            })
        })
        .or_else(|| {
            entities.iter().find(|entity| {
                entity.entity_type == "society"
                    && entity
                        .searchable_text
                        .to_ascii_lowercase()
                        .contains(&normalized)
            })
        })
        .ok_or_else(|| format!("no serving entity matched target {target:?}").into())
}

fn build_graph(
    bundle_version: &str,
    target: &ServingEntityRecord,
    entities: &[ServingEntityRecord],
    facts: &[ServingFactRecord],
    edges: &[ServingEdgeRecord],
    depth: usize,
    max_facts_per_entity: usize,
) -> DashboardGraph {
    let entity_by_id = entities
        .iter()
        .map(|entity| (entity.entity_id.as_str(), entity))
        .collect::<HashMap<_, _>>();
    let facts_by_entity = facts_by_entity(facts);
    let selected_entity_ids = connected_entity_ids(&target.entity_id, edges, depth);

    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut included_entity_ids = HashSet::new();
    let mut proximity_edge_count = 0_usize;

    for entity_id in &selected_entity_ids {
        let Some(entity) = entity_by_id.get(entity_id.as_str()) else {
            continue;
        };
        let coords = entity_coordinates(entity.entity_id.as_str(), &facts_by_entity);
        nodes.push(DashboardNode {
            id: entity.entity_id.clone(),
            label: entity.name.clone(),
            kind: "entity".to_string(),
            group: entity.entity_type.clone(),
            detail: entity.searchable_text.chars().take(320).collect(),
            latitude: coords.map(|coords| coords.0),
            longitude: coords.map(|coords| coords.1),
            source_type: entity.root_source.clone(),
        });
        included_entity_ids.insert(entity.entity_id.clone());
    }

    for edge in edges {
        if included_entity_ids.contains(&edge.from_entity_id)
            && included_entity_ids.contains(&edge.to_entity_id)
        {
            if edge.edge_type == "near_place" && edge.source_type == "Derived" {
                proximity_edge_count += 1;
            }
            links.push(DashboardLink {
                source: edge.from_entity_id.clone(),
                target: edge.to_entity_id.clone(),
                label: edge.edge_type.clone(),
                confidence: edge.confidence,
                source_type: edge.source_type.clone(),
            });
        }
    }

    let mut fact_count = 0_usize;
    let mut derived_fact_count = 0_usize;
    for entity_id in selected_entity_ids {
        let Some(entity_facts) = facts_by_entity.get(entity_id.as_str()) else {
            continue;
        };
        let mut selected_facts = entity_facts.to_vec();
        selected_facts.sort_by_key(|fact| fact_priority(fact));
        selected_facts.truncate(max_facts_per_entity);
        for (index, fact) in selected_facts.iter().enumerate() {
            let fact_id = format!("fact:{}:{index}", entity_id.replace(':', "_"));
            let group = fact_group(&fact.fact_key);
            if fact.source_type == "Derived" {
                derived_fact_count += 1;
            }
            fact_count += 1;
            nodes.push(DashboardNode {
                id: fact_id.clone(),
                label: fact.fact_key.clone(),
                kind: "fact".to_string(),
                group,
                detail: fact_detail(fact),
                latitude: None,
                longitude: None,
                source_type: Some(fact.source_type.clone()),
            });
            links.push(DashboardLink {
                source: entity_id.clone(),
                target: fact_id,
                label: "has_fact".to_string(),
                confidence: fact.confidence,
                source_type: fact.source_type.clone(),
            });
        }
    }

    DashboardGraph {
        bundle_version: bundle_version.to_string(),
        target_entity_id: target.entity_id.clone(),
        target_name: target.name.clone(),
        stats: DashboardStats {
            entity_count: included_entity_ids.len(),
            fact_count,
            edge_count: links.len(),
            proximity_edge_count,
            derived_fact_count,
        },
        nodes,
        links,
    }
}

fn facts_by_entity(facts: &[ServingFactRecord]) -> HashMap<&str, Vec<&ServingFactRecord>> {
    let mut by_entity = HashMap::<&str, Vec<&ServingFactRecord>>::new();
    for fact in facts {
        by_entity
            .entry(fact.entity_id.as_str())
            .or_default()
            .push(fact);
    }
    by_entity
}

fn connected_entity_ids(target: &str, edges: &[ServingEdgeRecord], depth: usize) -> Vec<String> {
    let mut adjacency = HashMap::<&str, Vec<&str>>::new();
    for edge in edges {
        adjacency
            .entry(edge.from_entity_id.as_str())
            .or_default()
            .push(edge.to_entity_id.as_str());
        adjacency
            .entry(edge.to_entity_id.as_str())
            .or_default()
            .push(edge.from_entity_id.as_str());
    }

    let mut seen = HashSet::<String>::new();
    let mut queue = VecDeque::from([(target.to_string(), 0_usize)]);
    while let Some((entity_id, current_depth)) = queue.pop_front() {
        if !seen.insert(entity_id.clone()) || current_depth >= depth {
            continue;
        }
        for next in adjacency.get(entity_id.as_str()).into_iter().flatten() {
            if !seen.contains(*next) {
                queue.push_back(((*next).to_string(), current_depth + 1));
            }
        }
    }
    let mut ids = seen.into_iter().collect::<Vec<_>>();
    ids.sort();
    ids
}

fn entity_coordinates(
    entity_id: &str,
    facts_by_entity: &HashMap<&str, Vec<&ServingFactRecord>>,
) -> Option<(f64, f64)> {
    let facts = facts_by_entity.get(entity_id)?;
    let latitude = numeric_fact(facts, "geo.latitude")?;
    let longitude = numeric_fact(facts, "geo.longitude")?;
    Some((latitude, longitude))
}

fn numeric_fact(facts: &[&ServingFactRecord], fact_key: &str) -> Option<f64> {
    facts
        .iter()
        .find(|fact| fact.fact_key == fact_key)
        .and_then(|fact| match &fact.value {
            FactValue::Numeric(value) => Some(*value),
            FactValue::Text(value) => value.parse().ok(),
            _ => None,
        })
}

fn fact_priority(fact: &ServingFactRecord) -> (u8, String) {
    let key = fact.fact_key.to_ascii_lowercase();
    let priority = if key.contains("high_voltage")
        || key.contains("stormwater")
        || key.contains("rajakaluve")
        || key.contains("lake")
    {
        0
    } else if key.starts_with("nearby_") || key.contains("groundwater") {
        1
    } else if key.starts_with("geo.") || fact.source_type == "Derived" {
        2
    } else if key.starts_with("rera_") || key.starts_with("project_") {
        3
    } else {
        4
    };
    (priority, key)
}

fn fact_group(fact_key: &str) -> String {
    let key = fact_key.to_ascii_lowercase();
    if key.contains("high_voltage")
        || key.contains("stormwater")
        || key.contains("rajakaluve")
        || key.contains("lake")
        || key.contains("graveyard")
    {
        "risk".to_string()
    } else if key.starts_with("nearby_") {
        "proximity".to_string()
    } else if key.starts_with("geo.") {
        "geo".to_string()
    } else if key.starts_with("rera_") || key.starts_with("project_") {
        "rera".to_string()
    } else {
        "fact".to_string()
    }
}

fn fact_detail(fact: &ServingFactRecord) -> String {
    let value = fact
        .value_text
        .clone()
        .unwrap_or_else(|| match &fact.value {
            FactValue::Numeric(value) => value.to_string(),
            FactValue::Text(value) => value.clone(),
            FactValue::Bool(value) => value.to_string(),
            FactValue::Tags(values) => values.join(", "),
            FactValue::Score { value, explanation } => format!("{value}: {explanation}"),
        });
    format!(
        "{}\n{}\nconfidence {:.2} · {}{}",
        fact.fact_key,
        value,
        fact.confidence,
        fact.source_type,
        fact.source_url
            .as_ref()
            .map(|url| format!(" · {url}"))
            .unwrap_or_default()
    )
}

fn render_html(graph: &DashboardGraph) -> Result<String, serde_json::Error> {
    let graph_json = serde_json::to_string(graph)?;
    Ok(format!(
        r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{target_name} Fact Graph</title>
  <style>
    :root {{
      color-scheme: light;
      --ink: #18212f;
      --muted: #627084;
      --line: #d8dee8;
      --bg: #f6f7f9;
      --panel: #ffffff;
      --risk: #c93f3f;
      --prox: #176b87;
      --entity: #243b6b;
      --fact: #667085;
      --good: #2f7d57;
    }}
    * {{ box-sizing: border-box; }}
    body {{ margin: 0; background: var(--bg); color: var(--ink); font: 13px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
    header {{ padding: 18px 22px 12px; border-bottom: 1px solid var(--line); background: var(--panel); }}
    h1 {{ margin: 0; font-size: 20px; letter-spacing: 0; }}
    .sub {{ color: var(--muted); margin-top: 4px; }}
    .shell {{ display: grid; grid-template-columns: minmax(560px, 1fr) 360px; gap: 14px; padding: 14px; min-height: calc(100vh - 72px); }}
    .panel {{ background: var(--panel); border: 1px solid var(--line); border-radius: 8px; overflow: hidden; }}
    .toolbar {{ display: flex; align-items: center; gap: 8px; padding: 10px; border-bottom: 1px solid var(--line); }}
    .toolbar button {{ border: 1px solid var(--line); background: #fff; border-radius: 6px; padding: 6px 9px; cursor: pointer; color: var(--ink); }}
    .toolbar button.active {{ background: #18212f; color: #fff; border-color: #18212f; }}
    svg {{ display: block; width: 100%; height: calc(100vh - 142px); min-height: 560px; background: #fbfcfe; }}
    .side {{ display: grid; grid-template-rows: auto auto 1fr; gap: 14px; }}
    .stats {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; padding: 10px; }}
    .stat {{ border: 1px solid var(--line); border-radius: 6px; padding: 8px; }}
    .stat b {{ display: block; font-size: 18px; }}
    .stat span {{ color: var(--muted); }}
    .details {{ padding: 12px; white-space: pre-wrap; overflow: auto; max-height: 300px; }}
    .list {{ overflow: auto; padding: 8px; }}
    .row {{ border-bottom: 1px solid #edf0f5; padding: 8px 4px; cursor: pointer; }}
    .row:hover {{ background: #f4f7fb; }}
    .row b {{ display: block; }}
    .row span {{ color: var(--muted); }}
    .legend {{ margin-left: auto; display: flex; gap: 10px; color: var(--muted); }}
    .dot {{ display: inline-block; width: 9px; height: 9px; border-radius: 50%; margin-right: 4px; }}
    .mapdot {{ opacity: .85; }}
    @media (max-width: 980px) {{ .shell {{ grid-template-columns: 1fr; }} svg {{ height: 620px; }} }}
  </style>
</head>
<body>
  <header>
    <h1>{target_name}</h1>
    <div class="sub">Serving bundle {bundle_version} · centered on {target_entity_id}</div>
  </header>
  <main class="shell">
    <section class="panel">
      <div class="toolbar">
        <button data-filter="all" class="active">All</button>
        <button data-filter="risk">Risk</button>
        <button data-filter="proximity">Proximity</button>
        <button data-filter="entity">Entities</button>
        <button id="restart">Recenter</button>
        <div class="legend">
          <span><i class="dot" style="background:var(--entity)"></i>entity</span>
          <span><i class="dot" style="background:var(--prox)"></i>proximity</span>
          <span><i class="dot" style="background:var(--risk)"></i>risk</span>
        </div>
      </div>
      <svg id="graph" role="img" aria-label="Fact graph"></svg>
    </section>
    <aside class="side">
      <section class="panel stats" id="stats"></section>
      <section class="panel">
        <div class="toolbar"><b>Selected</b></div>
        <div class="details" id="details"></div>
      </section>
      <section class="panel">
        <div class="toolbar"><b>Facts and Components</b></div>
        <div class="list" id="list"></div>
      </section>
    </aside>
  </main>
  <script>
    const data = {graph_json};
    const svg = document.getElementById('graph');
    const stats = document.getElementById('stats');
    const details = document.getElementById('details');
    const list = document.getElementById('list');
    const colors = {{ entity: '#243b6b', society: '#243b6b', place: '#176b87', road_segment: '#7d5a20', risk: '#c93f3f', proximity: '#176b87', geo: '#2f7d57', rera: '#6b5fb5', fact: '#667085' }};
    let activeFilter = 'all';
    let nodes = data.nodes.map((n, i) => ({{...n, x: 120 + (i % 12) * 46, y: 100 + Math.floor(i / 12) * 42, vx: 0, vy: 0}}));
    let nodeById = new Map(nodes.map(n => [n.id, n]));
    let links = data.links.map(l => ({{...l, sourceNode: nodeById.get(l.source), targetNode: nodeById.get(l.target)}})).filter(l => l.sourceNode && l.targetNode);
    const target = nodeById.get(data.target_entity_id);

    stats.innerHTML = [
      ['Entities', data.stats.entity_count],
      ['Facts shown', data.stats.fact_count],
      ['Links', data.stats.edge_count],
      ['R-tree proximity', data.stats.proximity_edge_count],
      ['Derived facts', data.stats.derived_fact_count],
      ['Bundle', data.bundle_version]
    ].map(([k,v]) => `<div class="stat"><b>${{v}}</b><span>${{k}}</span></div>`).join('');

    function visibleNode(n) {{
      if (activeFilter === 'all') return true;
      if (activeFilter === 'entity') return n.kind === 'entity';
      return n.group === activeFilter || (activeFilter === 'proximity' && n.source_type === 'Derived');
    }}
    function visibleLink(l) {{ return visibleNode(l.sourceNode) && visibleNode(l.targetNode); }}
    function radius(n) {{
      if (n.id === data.target_entity_id) return 18;
      if (n.kind === 'entity') return 12;
      return n.group === 'risk' ? 8 : 6;
    }}
    function color(n) {{ return colors[n.group] || colors[n.kind] || colors.fact; }}
    function escapeHtml(s) {{ return String(s ?? '').replace(/[&<>"']/g, c => ({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[c])); }}
    function showDetails(n) {{
      details.textContent = `${{n.label}}\n${{n.id}}\n${{n.group}} · ${{n.kind}}\n\n${{n.detail || ''}}`;
    }}
    function renderList() {{
      const items = nodes.filter(visibleNode).slice().sort((a,b) => (a.kind === 'entity' ? 0 : 1) - (b.kind === 'entity' ? 0 : 1) || a.group.localeCompare(b.group) || a.label.localeCompare(b.label));
      list.innerHTML = items.map(n => `<div class="row" data-id="${{escapeHtml(n.id)}}"><b>${{escapeHtml(n.label)}}</b><span>${{escapeHtml(n.group)}} · ${{escapeHtml(n.kind)}}</span></div>`).join('');
      list.querySelectorAll('.row').forEach(row => row.onclick = () => showDetails(nodeById.get(row.dataset.id)));
    }}
    function tick() {{
      const w = svg.clientWidth || 900, h = svg.clientHeight || 620;
      if (target) {{ target.x += (w * 0.5 - target.x) * 0.08; target.y += (h * 0.5 - target.y) * 0.08; }}
      for (const l of links) {{
        const desired = l.label === 'has_fact' ? 94 : 150;
        const dx = l.targetNode.x - l.sourceNode.x, dy = l.targetNode.y - l.sourceNode.y;
        const d = Math.max(1, Math.hypot(dx, dy));
        const force = (d - desired) * 0.012;
        const fx = force * dx / d, fy = force * dy / d;
        l.sourceNode.vx += fx; l.sourceNode.vy += fy;
        l.targetNode.vx -= fx; l.targetNode.vy -= fy;
      }}
      for (let i = 0; i < nodes.length; i++) {{
        for (let j = i + 1; j < nodes.length; j++) {{
          const a = nodes[i], b = nodes[j];
          const dx = b.x - a.x, dy = b.y - a.y;
          const d2 = Math.max(25, dx*dx + dy*dy);
          const f = 90 / d2;
          a.vx -= f * dx; a.vy -= f * dy; b.vx += f * dx; b.vy += f * dy;
        }}
      }}
      for (const n of nodes) {{
        n.vx *= 0.82; n.vy *= 0.82;
        n.x = Math.max(24, Math.min(w - 24, n.x + n.vx));
        n.y = Math.max(24, Math.min(h - 24, n.y + n.vy));
      }}
      draw();
      requestAnimationFrame(tick);
    }}
    function draw() {{
      const shownLinks = links.filter(visibleLink);
      const shownNodes = nodes.filter(visibleNode);
      const edgeMarkup = shownLinks.map(l => `<line x1="${{l.sourceNode.x}}" y1="${{l.sourceNode.y}}" x2="${{l.targetNode.x}}" y2="${{l.targetNode.y}}" stroke="${{l.source_type === 'Derived' ? '#176b87' : '#c9d1df'}}" stroke-width="${{l.label === 'near_place' ? 2.2 : 1}}" opacity="${{l.label === 'has_fact' ? 0.32 : 0.72}}"><title>${{escapeHtml(l.label)}} · ${{l.confidence.toFixed(2)}} · ${{escapeHtml(l.source_type)}}</title></line>`).join('');
      const nodeMarkup = shownNodes.map(n => `<g class="node" data-id="${{escapeHtml(n.id)}}"><circle cx="${{n.x}}" cy="${{n.y}}" r="${{radius(n)}}" fill="${{color(n)}}" stroke="${{n.id === data.target_entity_id ? '#111827' : '#fff'}}" stroke-width="2"><title>${{escapeHtml(n.label)}}\n${{escapeHtml(n.detail)}}</title></circle><text x="${{n.x + radius(n) + 4}}" y="${{n.y + 4}}" font-size="11" fill="#263241">${{escapeHtml(n.label).slice(0, 42)}}</text></g>`).join('');
      const geoNodes = nodes.filter(n => n.latitude && n.longitude && visibleNode(n));
      let mapMarkup = '';
      if (geoNodes.length) {{
        const lats = geoNodes.map(n => n.latitude), lngs = geoNodes.map(n => n.longitude);
        const minLat = Math.min(...lats), maxLat = Math.max(...lats), minLng = Math.min(...lngs), maxLng = Math.max(...lngs);
        const box = {{x: 18, y: 18, w: 180, h: 130}};
        mapMarkup += `<rect x="${{box.x}}" y="${{box.y}}" width="${{box.w}}" height="${{box.h}}" rx="7" fill="#fff" stroke="#d8dee8"/><text x="${{box.x + 10}}" y="${{box.y + 20}}" font-size="11" fill="#627084">geo plot</text>`;
        for (const n of geoNodes) {{
          const x = box.x + 12 + ((n.longitude - minLng) / Math.max(0.0001, maxLng - minLng)) * (box.w - 24);
          const y = box.y + box.h - 12 - ((n.latitude - minLat) / Math.max(0.0001, maxLat - minLat)) * (box.h - 34);
          mapMarkup += `<circle class="mapdot" cx="${{x}}" cy="${{y}}" r="${{n.id === data.target_entity_id ? 5 : 3}}" fill="${{color(n)}}"><title>${{escapeHtml(n.label)}}\n${{n.latitude}}, ${{n.longitude}}</title></circle>`;
        }}
      }}
      svg.innerHTML = edgeMarkup + nodeMarkup + mapMarkup;
      svg.querySelectorAll('.node').forEach(el => el.onclick = () => showDetails(nodeById.get(el.dataset.id)));
    }}
    document.querySelectorAll('[data-filter]').forEach(button => button.onclick = () => {{
      document.querySelectorAll('[data-filter]').forEach(b => b.classList.remove('active'));
      button.classList.add('active');
      activeFilter = button.dataset.filter;
      renderList();
      draw();
    }});
    document.getElementById('restart').onclick = () => {{
      const w = svg.clientWidth || 900, h = svg.clientHeight || 620;
      nodes.forEach((n, i) => {{ n.x = 120 + (i % 12) * 46; n.y = 100 + Math.floor(i / 12) * 42; n.vx = 0; n.vy = 0; }});
      if (target) {{ target.x = w * 0.5; target.y = h * 0.5; }}
    }};
    renderList();
    showDetails(target || nodes[0]);
    tick();
  </script>
</body>
</html>"##,
        target_name = html_escape(&graph.target_name),
        bundle_version = html_escape(&graph.bundle_version),
        target_entity_id = html_escape(&graph.target_entity_id),
        graph_json = graph_json,
    ))
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Debug)]
struct CliOptions {
    project_root: Option<PathBuf>,
    version: Option<String>,
    target: String,
    output: PathBuf,
    depth: usize,
    max_facts_per_entity: usize,
}

impl CliOptions {
    fn parse() -> Result<Self, String> {
        let mut args = std::env::args().skip(1);
        let mut options = Self {
            project_root: None,
            version: None,
            target: DEFAULT_TARGET.to_string(),
            output: PathBuf::from(DEFAULT_OUTPUT),
            depth: 2,
            max_facts_per_entity: 36,
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--project-root" => {
                    options.project_root =
                        Some(PathBuf::from(next_arg(&mut args, "--project-root")?));
                }
                "--version" => {
                    options.version = Some(next_arg(&mut args, "--version")?);
                }
                "--target" => {
                    options.target = next_arg(&mut args, "--target")?;
                }
                "--out" => {
                    options.output = PathBuf::from(next_arg(&mut args, "--out")?);
                }
                "--depth" => {
                    options.depth = next_arg(&mut args, "--depth")?
                        .parse()
                        .map_err(|_| "--depth requires a positive integer".to_string())?;
                }
                "--max-facts-per-entity" => {
                    options.max_facts_per_entity = next_arg(&mut args, "--max-facts-per-entity")?
                        .parse()
                        .map_err(|_| {
                            "--max-facts-per-entity requires a positive integer".to_string()
                        })?;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(options)
    }
}

fn next_arg(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value"))
}

fn print_help() {
    println!("Build a self-contained HTML fact graph dashboard from the current serving bundle.");
    println!();
    println!("Usage:");
    println!("  cargo run --bin openestates-fact-graph-dashboard -- [--target <entity id or name>] [--out <html>] [--depth N]");
    println!();
    println!("Defaults:");
    println!("  --target \"prestige waterford\"");
    println!("  --out {DEFAULT_OUTPUT}");
}

#[derive(Serialize)]
struct DashboardGraph {
    bundle_version: String,
    target_entity_id: String,
    target_name: String,
    stats: DashboardStats,
    nodes: Vec<DashboardNode>,
    links: Vec<DashboardLink>,
}

#[derive(Serialize)]
struct DashboardStats {
    entity_count: usize,
    fact_count: usize,
    edge_count: usize,
    proximity_edge_count: usize,
    derived_fact_count: usize,
}

#[derive(Serialize)]
struct DashboardNode {
    id: String,
    label: String,
    kind: String,
    group: String,
    detail: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    source_type: Option<String>,
}

#[derive(Serialize)]
struct DashboardLink {
    source: String,
    target: String,
    label: String,
    confidence: f32,
    source_type: String,
}
