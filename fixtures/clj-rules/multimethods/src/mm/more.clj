(ns mm.more
  "A defmethod in a different namespace from its defmulti."
  (:require [mm.shapes :as shapes]))

(defmethod shapes/area :triangle [{:keys [b h]}] (/ (* b h) 2))
