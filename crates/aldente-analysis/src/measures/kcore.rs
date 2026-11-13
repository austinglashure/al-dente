//! k-core numbers on the undirected simple projection (Batagelj and
//! Zaversnik, linear time).

/// The core number of every node, given undirected neighbor lists.
///
/// Example: a triangle with one pendant node gives `[2, 2, 2, 1]`.
pub fn core_numbers(adj: &[Vec<u32>]) -> Vec<u32> {
    let n = adj.len();
    let mut deg: Vec<usize> = adj.iter().map(Vec::len).collect();
    let max = deg.iter().copied().max().unwrap_or(0);
    // Bucket sort nodes by degree.
    let mut bin = vec![0usize; max + 1];
    for &d in &deg {
        bin[d] += 1;
    }
    let mut start = 0;
    for b in bin.iter_mut() {
        let count = *b;
        *b = start;
        start += count;
    }
    let mut pos = vec![0usize; n];
    let mut vert = vec![0usize; n];
    for v in 0..n {
        pos[v] = bin[deg[v]];
        vert[pos[v]] = v;
        bin[deg[v]] += 1;
    }
    for d in (1..=max).rev() {
        bin[d] = bin[d - 1];
    }
    if max > 0 || n > 0 {
        bin[0] = 0;
    }
    for i in 0..n {
        let v = vert[i];
        for &u in &adj[v] {
            let u = u as usize;
            if deg[u] > deg[v] {
                let du = deg[u];
                let pu = pos[u];
                let pw = bin[du];
                let w = vert[pw];
                if u != w {
                    pos[u] = pw;
                    vert[pu] = w;
                    pos[w] = pu;
                    vert[pw] = u;
                }
                bin[du] += 1;
                deg[u] -= 1;
            }
        }
    }
    deg.into_iter().map(|d| d as u32).collect()
}
