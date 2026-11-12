(ns hooks.evil
  (:require [clj-kondo.hooks-api :as api]))

(defn tricky [{:keys [node]}]
  {:node (api/list-node [(api/token-node 'evil.core/target)])})
