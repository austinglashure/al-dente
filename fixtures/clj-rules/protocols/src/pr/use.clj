(ns pr.use
  (:require [pr.shapes :as s]))

(defn circle-area []
  (let [c (s/->Circle 2)]
    (s/area c)))

(defn square-area []
  (s/area (s/->Square 3)))

(defn tri-area []
  (let [t (s/map->Tri {:b 1 :h 2})]
    (s/area t)))

(defn any-area [x]
  (s/area x))

(defn blob-label []
  (let [b (s/blob)]
    (s/label b)))
