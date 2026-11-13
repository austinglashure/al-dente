#!/usr/bin/env bash
# Regenerates wrap-cookies-duration.txt by running ring-core's own code at
# the acceptance commit (Austin approved, 2026-11-13). Usage: gen.sh <ring clone>
set -euo pipefail
cd "$(dirname "$0")"
RING="${1:?path to the ring clone}"
M2="$HOME/.m2/repository"
CP="$RING/ring-core/src:$M2/ring/ring-codec/1.3.0/ring-codec-1.3.0.jar:$M2/org/clojure/clojure/1.12.0/clojure-1.12.0.jar:$M2/org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar:$M2/org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar"
java -cp "$CP" clojure.main gen.clj 2> wrap-cookies-duration.txt
