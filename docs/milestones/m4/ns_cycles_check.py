"""Independent namespace-cycle check for M4 (WP4.7).

Reads clj-kondo's JSON analysis (namespace-definitions and namespace-usages,
produced with an empty --config-dir so nothing from the repository is
loaded) and finds strongly connected components of the require graph with
networkx. Al Dente's own pipeline is not used.

Usage: python -I ns_cycles_check.py <kondo-analysis.json>
"""

import json
import sys

import networkx as nx


def main(path: str) -> None:
    """Prints each namespace cycle (sorted) and the count.

    Example: a repository where a requires b and b requires a prints `['a', 'b']`.
    """
    analysis = json.load(open(path))["analysis"]
    defined = {d["name"] for d in analysis.get("namespace-definitions", [])}
    g = nx.DiGraph()
    g.add_nodes_from(defined)
    for u in analysis.get("namespace-usages", []):
        if u["from"] in defined and u["to"] in defined and u["from"] != u["to"]:
            g.add_edge(u["from"], u["to"])
    cycles = sorted(sorted(c) for c in nx.strongly_connected_components(g) if len(c) > 1)
    for c in cycles:
        print(c)
    print(f"{len(cycles)} namespace cycles over {g.number_of_nodes()} namespaces, {g.number_of_edges()} requires")


if __name__ == "__main__":
    main(sys.argv[1])
