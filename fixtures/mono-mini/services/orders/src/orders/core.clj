(ns orders.core
  (:require [common.util :as util]
            [config]))

(defn total [items]
  (util/sum (map :price items)))

(defn port [] config/port)

(defn charge! [amount]
  (util/sum [amount]))
