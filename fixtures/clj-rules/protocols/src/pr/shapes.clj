(ns pr.shapes)

(defprotocol Shape
  (area [s])
  (label [s]))

(defrecord Circle [r]
  Shape
  (area [_] (* 3 r r))
  (label [_] (str "circle")))

(deftype Square [side]
  Shape
  (area [_] (* side side))
  (label [_] "square"))

(defrecord Tri [b h])

(extend-type Tri
  Shape
  (area [t] (/ (* (:b t) (:h t)) 2))
  (label [_] "tri"))

(extend-protocol Shape
  nil
  (area [_] 0)
  (label [_] "nothing")
  Object
  (area [_] -1)
  (label [_] "unknown")
  java.util.Map
  (area [m] (get m :area 0))
  (label [_] "map"))

(defn blob []
  (reify Shape
    (area [_] (inc 41))
    (label [_] "blob")))
