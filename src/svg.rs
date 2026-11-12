//! Standalone, deterministic SVG diagrams; rendering never changes query counters.

use crate::{VebSet, veb::Node};
use std::{collections::BTreeSet, fmt::Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query {
    Predecessor(u32),
    Successor(u32),
}

#[derive(Debug, Clone, Copy)]
pub struct SvgOptions {
    pub max_depth: usize,
    pub max_nodes: usize,
    pub show_empty: bool,
    pub query: Option<Query>,
}
impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_nodes: 200,
            show_empty: false,
            query: None,
        }
    }
}

const CARD_W: usize = 232;
const CARD_H: usize = 122;
const GAP_X: usize = 22;
const GAP_Y: usize = 178;
const TOP: usize = 192;

struct Card {
    id: String,
    label: String,
    universe: u32,
    min: Option<u32>,
    max: Option<u32>,
    offset: u32,
    summary_domain: bool,
    summary_edge: bool,
    note: String,
    children: Vec<Card>,
    width: usize,
    x: usize,
    y: usize,
}

struct Builder {
    options: SvgOptions,
    remaining: usize,
    deepest: usize,
}

#[derive(Clone, Copy)]
struct Domain {
    offset: u32,
    summary: bool,
}

impl Builder {
    fn card(
        &mut self,
        node: &Node,
        id: String,
        label: String,
        domain: Domain,
        depth: usize,
        summary_edge: bool,
    ) -> Card {
        self.remaining -= 1;
        self.deepest = self.deepest.max(depth);
        let mut card = Card {
            id,
            label,
            universe: node.universe,
            min: node.min,
            max: node.max,
            offset: domain.offset,
            summary_domain: domain.summary,
            summary_edge,
            note: String::new(),
            children: Vec::new(),
            width: CARD_W,
            x: 0,
            y: TOP + depth * GAP_Y,
        };
        let mut eligible = 0;
        let mut omitted = 0;
        let mut empty = 0;
        if let Some(summary) = &node.summary {
            let mut candidates = vec![(
                summary.as_ref(),
                "s".to_string(),
                "SUMMARY".to_string(),
                Domain {
                    offset: 0,
                    summary: true,
                },
                true,
            )];
            candidates.extend(node.clusters.iter().enumerate().map(|(index, child)| {
                (
                    child,
                    format!("c{index}"),
                    format!("CLUSTER {index}"),
                    Domain {
                        offset: domain.offset + node.index(index, 0),
                        ..domain
                    },
                    false,
                )
            }));
            for (child, suffix, label, child_domain, is_summary) in candidates {
                if !self.options.show_empty && child.min.is_none() {
                    empty += 1;
                    continue;
                }
                eligible += 1;
                if depth >= self.options.max_depth || self.remaining == 0 {
                    omitted += 1;
                    continue;
                }
                card.children.push(self.card(
                    child,
                    format!("{}/{suffix}", card.id),
                    label,
                    child_domain,
                    depth + 1,
                    is_summary,
                ));
            }
        }
        if empty > 0 {
            write!(card.note, "{empty} empty branches hidden").unwrap();
        }
        if omitted > 0 {
            if !card.note.is_empty() {
                card.note.push_str("; ");
            }
            write!(card.note, "{omitted}/{eligible} branches collapsed").unwrap();
        }
        if card.note.is_empty() {
            card.note = if node.universe == 2 {
                "base case"
            } else {
                "minimum stored outside clusters"
            }
            .into();
        }
        if !card.children.is_empty() {
            card.width = card.children.iter().map(|c| c.width).sum::<usize>()
                + GAP_X * (card.children.len() - 1);
            card.width = card.width.max(CARD_W);
        }
        card
    }
}

impl Card {
    fn position(&mut self, left: usize) {
        self.x = left + (self.width - CARD_W) / 2;
        let mut child_left = left;
        for child in &mut self.children {
            child.position(child_left);
            child_left += child.width + GAP_X;
        }
    }

    fn edges(&self, svg: &mut String, trace: &BTreeSet<String>) {
        for child in &self.children {
            let x1 = self.x + CARD_W / 2;
            let y1 = self.y + CARD_H;
            let x2 = child.x + CARD_W / 2;
            let y2 = child.y;
            let color = if trace.contains(&self.id) && trace.contains(&child.id) {
                "#db7d23"
            } else if child.summary_edge {
                "#9a79c8"
            } else {
                "#79a49b"
            };
            let dash = if child.summary_edge {
                " stroke-dasharray=\"6 5\""
            } else {
                ""
            };
            writeln!(svg, "<path d=\"M{x1},{y1} C{x1},{} {x2},{} {x2},{y2}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"2\"{dash}/>", y1 + 28, y2 - 28).unwrap();
            child.edges(svg, trace);
        }
    }

    fn paint(&self, svg: &mut String, trace: &BTreeSet<String>) {
        let (fill, border) = if self.id == "r" {
            ("#eaf3fb", "#759ab7")
        } else if self.summary_domain {
            ("#f5efff", "#b197d4")
        } else if self.min.is_none() {
            ("#f1f4f6", "#c7d0d6")
        } else {
            ("#eff8f4", "#8db9a8")
        };
        let (border, stroke) = if trace.contains(&self.id) {
            ("#db7d23", 3)
        } else {
            (border, 1)
        };
        writeln!(svg, "<g id=\"{}\"><title>{}: local universe {}, min {:?}, max {:?}</title><rect x=\"{}\" y=\"{}\" width=\"{CARD_W}\" height=\"{CARD_H}\" rx=\"10\" fill=\"{fill}\" stroke=\"{border}\" stroke-width=\"{stroke}\"/>", self.id.replace('/', "-"), self.label, self.universe, self.min, self.max, self.x, self.y).unwrap();
        let x = self.x + 14;
        text(
            svg,
            x,
            self.y + 23,
            12,
            "#31546a",
            &format!("{}  /  U={}", self.label, self.universe),
        );
        let domain = if self.summary_domain {
            "summary IDs"
        } else {
            "root keys"
        };
        text(
            svg,
            x,
            self.y + 44,
            11,
            "#526575",
            &format!(
                "{domain} [{}..{})",
                self.offset,
                self.offset + self.universe
            ),
        );
        text(
            svg,
            x,
            self.y + 66,
            13,
            "#19364a",
            &format!("local min {}   max {}", value(self.min), value(self.max)),
        );
        let global = if self.summary_domain {
            "summary keys are cluster indices".into()
        } else {
            format!(
                "root min {}   max {}",
                value(self.min.map(|v| v + self.offset)),
                value(self.max.map(|v| v + self.offset))
            )
        };
        text(svg, x, self.y + 85, 11, "#526575", &global);
        // Long collapse notes use two short lines to stay inside the card.
        for (line, part) in self.note.split("; ").enumerate() {
            text(svg, x, self.y + 103 + line * 13, 10, "#526575", part);
        }
        svg.push_str("</g>\n");
        for child in &self.children {
            child.paint(svg, trace);
        }
    }
}

fn value(value: Option<u32>) -> String {
    value.map_or_else(|| "-".into(), |v| v.to_string())
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn text(svg: &mut String, x: usize, y: usize, size: usize, color: &str, contents: &str) {
    writeln!(
        svg,
        "<text x=\"{x}\" y=\"{y}\" font-size=\"{size}\" fill=\"{color}\">{}</text>",
        escape(contents)
    )
    .unwrap();
}

// Mirrors the query's decisions while recording both recursive calls and cached-extremum reads.
// Kept separate from timed algorithms so diagram collection adds no query overhead.
fn trace(node: &Node, query: Query, path: &str, visited: &mut BTreeSet<String>) -> Option<u32> {
    visited.insert(path.into());
    let (key, successor) = match query {
        Query::Successor(key) => (key, true),
        Query::Predecessor(key) => (key, false),
    };
    let (min, max) = (node.min?, node.max?);
    if successor {
        if key < min {
            return Some(min);
        }
        if key >= max {
            return None;
        }
    } else {
        if key > max {
            return Some(max);
        }
        if key <= min {
            return None;
        }
    }
    if node.universe == 2 {
        return if successor { node.max } else { node.min };
    }
    let high = node.high(key);
    let low = node.low(key);
    let cluster = &node.clusters[high];
    let child_path = format!("{path}/c{high}");
    visited.insert(child_path.clone());
    let same_cluster = if successor {
        cluster.max.is_some_and(|v| low < v)
    } else {
        cluster.min.is_some_and(|v| low > v)
    };
    let make_query = |key| {
        if successor {
            Query::Successor(key)
        } else {
            Query::Predecessor(key)
        }
    };
    if same_cluster {
        return trace(cluster, make_query(low), &child_path, visited).map(|v| node.index(high, v));
    }
    let next = trace(
        node.summary.as_ref().unwrap(),
        make_query(high as u32),
        &format!("{path}/s"),
        visited,
    );
    match next {
        Some(next) => {
            visited.insert(format!("{path}/c{next}"));
            let cluster = &node.clusters[next as usize];
            Some(node.index(
                next as usize,
                if successor {
                    cluster.min.unwrap()
                } else {
                    cluster.max.unwrap()
                },
            ))
        }
        None if successor => None,
        None => node.min,
    }
}

impl VebSet {
    /// Renders at most `max_nodes` cards. Empty/limited branches are explicitly labelled.
    pub fn to_svg(&self, options: SvgOptions) -> Result<String, String> {
        if options.max_nodes == 0 {
            return Err("max_nodes must be positive".into());
        }
        let mut visited = BTreeSet::new();
        let query_label = if let Some(query) = options.query {
            let key = match query {
                Query::Predecessor(k) | Query::Successor(k) => k,
            };
            if key >= self.universe() {
                return Err(format!("query key {key} is outside 0..{}", self.universe()));
            }
            let result = trace(&self.root, query, "r", &mut visited);
            format!(
                "{query:?} = {}  |  orange: accessed nodes (including extrema reads)",
                value(result)
            )
        } else {
            "Purple dashed: summary  |  green solid: cluster  |  extrema use local coordinates"
                .into()
        };
        let mut builder = Builder {
            options,
            remaining: options.max_nodes,
            deepest: 0,
        };
        let mut root = builder.card(
            &self.root,
            "r".into(),
            "ROOT".into(),
            Domain {
                offset: 0,
                summary: false,
            },
            0,
            false,
        );
        let width = (root.width + 64).max(1024);
        root.position((width - root.width) / 2);
        let height = TOP + builder.deepest * GAP_Y + CARD_H + 48;
        let storage = self.storage();
        let shown = options.max_nodes - builder.remaining;
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-labelledby=\"title description\">\n<title id=\"title\">van Emde Boas tree, universe {}</title>\n<desc id=\"description\">Recursive cluster hierarchy. Summary nodes store cluster indices. Highlighted nodes show query accesses; collapsed nodes may hide accesses.</desc>\n<rect width=\"100%\" height=\"100%\" fill=\"#fbfcfe\"/>\n<g font-family=\"Segoe UI,Arial,sans-serif\">\n",
            self.universe()
        );
        text(
            &mut svg,
            32,
            40,
            12,
            "#426782",
            "INTEGER SETS / RECURSIVE DECOMPOSITION",
        );
        text(
            &mut svg,
            32,
            79,
            30,
            "#163c55",
            &format!("van Emde Boas / universe {}", self.universe()),
        );
        text(
            &mut svg,
            32,
            108,
            14,
            "#526575",
            &format!(
                "{} keys  |  {} allocated nodes  |  {} owned bytes  |  {shown} cards shown",
                self.len(),
                storage.allocated_nodes,
                storage.owned_bytes
            ),
        );
        text(&mut svg, 32, 138, 13, "#92602d", &query_label);
        text(
            &mut svg,
            32,
            161,
            11,
            "#526575",
            "Summary domains are independent of root keys. Collapsed branches may contain highlighted accesses.",
        );
        root.edges(&mut svg, &visited);
        root.paint(&mut svg, &visited);
        svg.push_str("</g>\n</svg>\n");
        Ok(svg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_matches_queries_for_every_small_universe_state() {
        let mut set = VebSet::new(16).unwrap();
        let mut previous = 0u32;
        let mut visited = BTreeSet::new();
        for i in 0u32..65_536 {
            let mask = i ^ (i >> 1);
            if i != 0 {
                let key = (mask ^ previous).trailing_zeros();
                if mask & (1 << key) != 0 {
                    set.insert(key).unwrap();
                } else {
                    set.remove(key).unwrap();
                }
            }
            previous = mask;
            for key in 0..16 {
                for query in [Query::Predecessor(key), Query::Successor(key)] {
                    visited.clear();
                    let result = trace(&set.root, query, "r", &mut visited);
                    let expected = match query {
                        Query::Predecessor(k) => set.predecessor(k).unwrap(),
                        Query::Successor(k) => set.successor(k).unwrap(),
                    };
                    assert_eq!(result, expected, "subset {mask:#06x}, {query:?}");
                }
            }
        }
    }
}
