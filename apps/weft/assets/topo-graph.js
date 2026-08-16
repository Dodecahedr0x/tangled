/* weft topology — d3-force graph. SSR-safe: no-op without window/d3. */
(function () {
  if (typeof window === "undefined" || typeof document === "undefined") return;

  function kindClass(kind, reachable) {
    if (kind === "home") return "topo-node home";
    if (kind === "hosted") return "topo-node hosted";
    return reachable === false ? "topo-node peer down" : "topo-node peer";
  }

  function radius(kind) {
    if (kind === "home") return 14;
    if (kind === "hosted") return 5;
    return 10;
  }

  function detailHtml(d) {
    if (!d) return '<p class="muted">Hover or click a node.</p>';
    var rows = [];
    function row(k, v) {
      if (v === undefined || v === null || v === "") return;
      rows.push(
        '<p class="did"><span class="field-label">' +
          k +
          "</span> " +
          String(v) +
          "</p>"
      );
    }
    var title =
      d.kind === "home" ? "Home knot" : d.kind === "hosted" ? "Identity" : "Peer knot";
    rows.push('<p class="page-title"><span class="tick"></span>' + title + "</p>");
    row("URL", d.url);
    row("DID", d.did);
    row("Handle", d.handle);
    row("Hosted on", d.hostedOn);
    if (d.kind !== "hosted") row("Reachable", d.reachable === false ? "no" : "yes");
    row("Identities", d.identities);
    row("Records", d.records);
    row("Peers", d.peers);
    row("Commits", d.commits);
    return rows.join("");
  }

  function showDetail(d) {
    var panel = document.getElementById("weft-topo-detail");
    if (panel) panel.innerHTML = detailHtml(d);
  }

  function render(el, data) {
    if (typeof d3 === "undefined" || !el || !data) return;
    var nodes = (data.nodes || []).map(function (n) {
      return Object.assign({}, n);
    });
    var links = (data.links || []).map(function (l) {
      return { source: l.source, target: l.target, kind: l.kind };
    });
    var w = el.clientWidth || 640;
    var h = el.clientHeight || 420;

    el.innerHTML = "";
    var svg = d3
      .select(el)
      .append("svg")
      .attr("class", "topo-graph")
      .attr("viewBox", [0, 0, w, h])
      .attr("role", "img")
      .attr("aria-label", "Network topology");
    svg.append("title").text("Network topology");

    var g = svg.append("g");
    svg.call(
      d3
        .zoom()
        .scaleExtent([0.35, 4])
        .on("zoom", function (ev) {
          g.attr("transform", ev.transform);
        })
    );

    var sim = d3
      .forceSimulation(nodes)
      .force(
        "link",
        d3
          .forceLink(links)
          .id(function (d) {
            return d.id;
          })
          .distance(function (d) {
            return d.kind === "hosted" ? 52 : 130;
          })
      )
      .force("charge", d3.forceManyBody().strength(-220))
      .force("center", d3.forceCenter(w / 2, h / 2))
      .force(
        "collide",
        d3.forceCollide().radius(function (d) {
          return radius(d.kind) + 10;
        })
      );

    var link = g
      .append("g")
      .selectAll("line")
      .data(links)
      .join("line")
      .attr("class", function (d) {
        return d.kind === "hosted" ? "topo-edge hosted" : "topo-edge";
      });

    var node = g
      .append("g")
      .selectAll("circle")
      .data(nodes)
      .join("circle")
      .attr("r", function (d) {
        return radius(d.kind);
      })
      .attr("class", function (d) {
        return kindClass(d.kind, d.reachable);
      })
      .attr("tabindex", 0)
      .on("mouseenter", function (_, d) {
        showDetail(d);
      })
      .on("click", function (_, d) {
        showDetail(d);
      })
      .on("keydown", function (ev, d) {
        if (ev.key === "Enter" || ev.key === " ") {
          ev.preventDefault();
          showDetail(d);
        }
      })
      .call(
        d3
          .drag()
          .on("start", function (ev, d) {
            if (!ev.active) sim.alphaTarget(0.2).restart();
            d.fx = d.x;
            d.fy = d.y;
          })
          .on("drag", function (ev, d) {
            d.fx = ev.x;
            d.fy = ev.y;
          })
          .on("end", function (ev, d) {
            if (!ev.active) sim.alphaTarget(0);
            d.fx = null;
            d.fy = null;
          })
      );

    var label = g
      .append("g")
      .selectAll("text")
      .data(nodes)
      .join("text")
      .attr("class", "topo-label")
      .attr("text-anchor", "middle")
      .text(function (d) {
        return d.label || "";
      });

    sim.on("tick", function () {
      link
        .attr("x1", function (d) {
          return d.source.x;
        })
        .attr("y1", function (d) {
          return d.source.y;
        })
        .attr("x2", function (d) {
          return d.target.x;
        })
        .attr("y2", function (d) {
          return d.target.y;
        });
      node
        .attr("cx", function (d) {
          return d.x;
        })
        .attr("cy", function (d) {
          return d.y;
        });
      label
        .attr("x", function (d) {
          return d.x;
        })
        .attr("y", function (d) {
          return d.y + radius(d.kind) + 12;
        });
    });

    if (nodes[0]) showDetail(nodes[0]);
  }

  function mount(el) {
    if (!el || typeof d3 === "undefined") return;
    var raw = el.getAttribute("data-graph");
    if (!raw || raw === el._weftGraph) return;
    el._weftGraph = raw;
    try {
      render(el, JSON.parse(raw));
    } catch (e) {
      el._weftGraph = null;
    }
  }

  function scan() {
    mount(document.getElementById("weft-topo"));
  }

  window.weftTopoMount = mount;

  if (typeof MutationObserver !== "undefined") {
    new MutationObserver(scan).observe(document.documentElement, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["data-graph"],
    });
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", scan);
  } else {
    scan();
  }
})();
