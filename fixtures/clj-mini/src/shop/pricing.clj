(ns shop.pricing
  "A multimethod with three methods and a default."
  (:require [shop.util :as util]))

(defmulti price
  "Price of an order, by its :kind."
  :kind)

(defn half-off [p]
  (/ p 2))

(defmethod price :book [order]
  (util/apply-discount half-off (:price order)))

(defmethod price :food [order]
  (* 1.1 (:price order)))

(defmethod price :gift [_order]
  0)

(defmethod price :default [order]
  (:price order))
