(ns billing.core
  (:require [config])
  (:gen-class))

(defn -main [& _args]
  (println "billing on" config/port))
