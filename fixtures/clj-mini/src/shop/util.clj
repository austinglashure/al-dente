(ns shop.util
  "Helpers: a private fn, a higher-order fn and a macro."
  (:require [shop.interop]))

(defn- clamp [lo hi x]
  (max lo (min hi x)))

(defn apply-discount
  "Calls its `rule` parameter on the price."
  [rule price]
  (clamp 0 price (rule price)))

(defn log-timing [label ms]
  (println label ms))

(defmacro with-timing
  "Runs body and logs how long it took."
  [label & body]
  `(let [start# (shop.interop/now-ms)
         result# (do ~@body)]
     (log-timing ~label (- (shop.interop/now-ms) start#))
     result#))
