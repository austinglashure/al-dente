//! Articulation points and bridges of the undirected simple projection
//! (Hopcroft–Tarjan, iterative).

/// `(articulation flags, bridges as (a, b) with a < b, sorted)`.
///
/// Example: on the path `0 – 1 – 2`, node 1 is an articulation point and
/// both edges are bridges.
pub fn articulation_and_bridges(adj: &[Vec<u32>]) -> (Vec<bool>, Vec<(u32, u32)>) {
    let n = adj.len();
    let mut disc = vec![u32::MAX; n];
    let mut low = vec![0u32; n];
    let mut is_cut = vec![false; n];
    let mut bridges = Vec::new();
    let mut time = 0u32;
    // (node, parent, next neighbor position, children)
    let mut stack: Vec<(usize, usize, usize, u32)> = Vec::new();
    for root in 0..n {
        if disc[root] != u32::MAX {
            continue;
        }
        disc[root] = time;
        low[root] = time;
        time += 1;
        stack.push((root, usize::MAX, 0, 0));
        while let Some(top) = stack.last_mut() {
            let (v, parent) = (top.0, top.1);
            if top.2 < adj[v].len() {
                let w = adj[v][top.2] as usize;
                top.2 += 1;
                if disc[w] == u32::MAX {
                    top.3 += 1;
                    disc[w] = time;
                    low[w] = time;
                    time += 1;
                    stack.push((w, v, 0, 0));
                } else if w != parent {
                    low[v] = low[v].min(disc[w]);
                }
                continue;
            }
            let children = top.3;
            stack.pop();
            if let Some(&(p, pp, _, _)) = stack.last() {
                low[p] = low[p].min(low[v]);
                if low[v] > disc[p] {
                    bridges.push((p.min(v) as u32, p.max(v) as u32));
                }
                if pp != usize::MAX && low[v] >= disc[p] {
                    is_cut[p] = true;
                }
            } else if children > 1 {
                is_cut[v] = true;
            }
        }
    }
    bridges.sort_unstable();
    (is_cut, bridges)
}
