#!/usr/bin/env bash
# Regenerates the clj-mini traces (see ../README.md). Runs the fixture's own
# code, never an analyzed repository's. Needs java and the Clojure 1.12.0 jars in ~/.m2.
set -euo pipefail
cd "$(dirname "$0")"
M2="$HOME/.m2/repository/org/clojure"
CP="../../../clj-mini/src:$M2/clojure/1.12.0/clojure-1.12.0.jar:$M2/spec.alpha/0.5.238/spec.alpha-0.5.238.jar:$M2/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar"
java -cp "$CP" clojure.main gen.clj
