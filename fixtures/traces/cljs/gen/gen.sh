#!/usr/bin/env bash
# Regenerates the ClojureScript trace fixtures (see ../../README.md): compiles
# src/ with ClojureScript 1.11.60 (:optimizations :none, source maps on),
# runs it under Node, and keeps app/core.js, its map and the Node trace with
# this machine's paths replaced by a fixed prefix.
set -euo pipefail
cd "$(dirname "$0")"
rm -rf out
clojure -Sdeps '{:deps {org.clojure/clojurescript {:mvn/version "1.11.60"}}}' \
  -M -m cljs.main -t node -O none -d out -c app.core
node out/main.js > node-raw.txt
here="$(pwd)"
sed "s#${here}/out/#/srv/app/out/#g" node-raw.txt > ../node.txt
rm node-raw.txt
mkdir -p ../out/app
cp out/app/core.js ../out/app/
# The map's "file" names this machine's absolute path; keep only the file name.
sed 's#"file":"[^"]*"#"file":"core.js"#' out/app/core.js.map > ../out/app/core.js.map
