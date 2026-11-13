#!/usr/bin/env bash
# Regenerates the JVM trace corpus from src/ (see ../README.md).
# Needs java and the Clojure 1.12.0 jars in ~/.m2.
set -euo pipefail
cd "$(dirname "$0")"
M2="$HOME/.m2/repository/org/clojure"
CP="src:$M2/clojure/1.12.0/clojure-1.12.0.jar:$M2/spec.alpha/0.5.238/spec.alpha-0.5.238.jar:$M2/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar"
java -cp "$CP" clojure.main gen.clj
# A REPL session: the "Execution error" line and clojure.repl/pst output.
printf "(require 'corpus.core)\n(corpus.core/wrap-twice)\n(clojure.repl/pst)\n" \
  | java -cp "$CP" clojure.main --repl 2>&1 | sed 's/^user=> //' > ../12-repl.txt
# A script that dies: clojure.main's error report as data.
java -Dclojure.main.report=stderr -cp "$CP" clojure.main -e "(require 'corpus.core) (corpus.core/wrap-ex-info)" 2> ../16-main-report.edn || true
