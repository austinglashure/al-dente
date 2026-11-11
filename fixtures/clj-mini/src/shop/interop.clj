(ns shop.interop
  "Java interop."
  (:import (java.util UUID)))

(defn now-ms []
  (System/currentTimeMillis))

(defn new-id []
  (str (UUID/randomUUID)))

(defn shout [^String s]
  (.toUpperCase s))
