(ns shop.audit
  "Only reached through requiring-resolve.")

(defn record! [event]
  (println "audit" event))
