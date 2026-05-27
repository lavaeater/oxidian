use dioxus::prelude::*;

/// Renders a force-directed graph on a canvas element.
/// `nodes`: list of (id, label, is_active)
/// `edges`: list of (source_id, target_id)
/// `on_select`: called with the node id when a node is clicked
#[component]
pub fn GraphView(
    nodes: Vec<(String, String, bool)>,
    edges: Vec<(String, String)>,
    on_select: EventHandler<String>,
) -> Element {
    let canvas_id = use_memo(|| {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        format!("graph-canvas-{}", N.fetch_add(1, Ordering::Relaxed))
    });

    // Serialize nodes + edges to JSON and drive the canvas via JS.
    let nodes_json = serde_json::to_string(&nodes.iter()
        .map(|(id, label, active)| serde_json::json!({"id": id, "label": label, "active": active}))
        .collect::<Vec<_>>())
        .unwrap_or_default();
    let edges_json = serde_json::to_string(&edges.iter()
        .map(|(s, t)| serde_json::json!({"s": s, "t": t}))
        .collect::<Vec<_>>())
        .unwrap_or_default();

    let cid = canvas_id();
    use_effect(move || {
        let js = format!(r#"
(function() {{
    const canvas = document.getElementById({cid:?});
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const W = canvas.width = canvas.offsetWidth || 400;
    const H = canvas.height = canvas.offsetHeight || 300;

    const nodes = {nodes_json};
    const edges = {edges_json};

    // Assign random initial positions
    nodes.forEach(n => {{
        n.x = Math.random() * W;
        n.y = Math.random() * H;
        n.vx = 0; n.vy = 0;
    }});
    const nodeMap = Object.fromEntries(nodes.map(n => [n.id, n]));

    function tick() {{
        // Repulsion
        for (let i = 0; i < nodes.length; i++) {{
            for (let j = i + 1; j < nodes.length; j++) {{
                const a = nodes[i], b = nodes[j];
                const dx = b.x - a.x, dy = b.y - a.y;
                const d = Math.sqrt(dx*dx + dy*dy) || 1;
                const f = 800 / (d * d);
                a.vx -= dx * f / d; a.vy -= dy * f / d;
                b.vx += dx * f / d; b.vy += dy * f / d;
            }}
        }}
        // Attraction along edges
        edges.forEach(e => {{
            const a = nodeMap[e.s], b = nodeMap[e.t];
            if (!a || !b) return;
            const dx = b.x - a.x, dy = b.y - a.y;
            const d = Math.sqrt(dx*dx + dy*dy) || 1;
            const f = (d - 80) * 0.03;
            a.vx += dx * f / d; a.vy += dy * f / d;
            b.vx -= dx * f / d; b.vy -= dy * f / d;
        }});
        // Centre gravity
        nodes.forEach(n => {{
            n.vx += (W/2 - n.x) * 0.01;
            n.vy += (H/2 - n.y) * 0.01;
            n.vx *= 0.85; n.vy *= 0.85;
            n.x = Math.max(20, Math.min(W-20, n.x + n.vx));
            n.y = Math.max(20, Math.min(H-20, n.y + n.vy));
        }});
    }}

    function draw() {{
        ctx.clearRect(0, 0, W, H);
        // Edges
        ctx.strokeStyle = 'rgba(100,130,180,0.35)';
        ctx.lineWidth = 1;
        edges.forEach(e => {{
            const a = nodeMap[e.s], b = nodeMap[e.t];
            if (!a || !b) return;
            ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
        }});
        // Nodes
        nodes.forEach(n => {{
            ctx.beginPath();
            ctx.arc(n.x, n.y, n.active ? 7 : 5, 0, Math.PI*2);
            ctx.fillStyle = n.active ? '#4493f8' : '#7d8590';
            ctx.fill();
            ctx.font = '10px system-ui';
            ctx.fillStyle = '#e6edf3';
            ctx.fillText(n.label.slice(0,18), n.x + 8, n.y + 4);
        }});
    }}

    let frame;
    let steps = 0;
    function loop() {{
        tick(); draw();
        if (++steps < 120) frame = requestAnimationFrame(loop);
        else draw(); // settle
    }}
    loop();

    // Click detection
    canvas._graphNodes = nodes;
    canvas.onclick = function(e) {{
        const rect = canvas.getBoundingClientRect();
        const mx = e.clientX - rect.left, my = e.clientY - rect.top;
        for (const n of canvas._graphNodes) {{
            if (Math.hypot(n.x - mx, n.y - my) < 10) {{
                dioxus.send(n.id);
                return;
            }}
        }}
    }};
}})();
"#);
        spawn(async move {
            // Small delay so the canvas is in the DOM
            let _ = document::eval("await new Promise(r => setTimeout(r, 50));").await;
            // Loop receiving node-click events sent via dioxus.send(nodeId)
            let mut graph_eval = document::eval(&js);
            loop {
                match graph_eval.recv::<String>().await {
                    Ok(id) if !id.is_empty() => on_select(id),
                    _ => break,
                }
            }
        });
    });

    rsx! {
        canvas {
            id: "{canvas_id}",
            class: "graph-canvas",
        }
    }
}
