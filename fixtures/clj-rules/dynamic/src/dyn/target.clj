(ns dyn.target)

(defn handler [] :handled)

(defn other [] :other)

(def ^:dynamic *config* {})

(def counter 0)
