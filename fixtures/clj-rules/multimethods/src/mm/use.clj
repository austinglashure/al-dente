(ns mm.use
  (:require [mm.more]
            [mm.shapes :as s]))

(defn circle-area [] (s/area {:shape :circle :r 2}))

(defn unknown-area [x] (s/area x))

(defn unmatched-area [] (s/area {:shape :hexagon}))

(defn pair [] (s/describe "a" "b"))

(defn french [] (s/greet {:lang :fr}))

(defn poodle [] (s/speak {:animal :mm.shapes/poodle}))

(defn as-value [] (map s/area [{:shape :square}]))
