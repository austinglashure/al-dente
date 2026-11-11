(ns shop.model
  "Shared by Clojure and ClojureScript."
  (:require [clojure.string :as str]))

(defn format-money [amount]
  #?(:clj (format "%.2f" (double amount))
     :cljs (str "$" amount)))

(defn order-summary [order total]
  (str/join " " [(:sku order) (format-money total)]))
