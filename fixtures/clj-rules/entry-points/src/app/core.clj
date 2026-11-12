(ns app.core
  (:require [ring.adapter.jetty :as jetty]))

(defn app [_req] {:status 200})

(defn other-handler [_req] {:status 404})

(defn start [_opts]
  (jetty/run-jetty #'app {:port 8080}))

(defn serve-other []
  (jetty/run-jetty other-handler {:port 8081}))

(defn -main [& _args]
  (start {}))

(defn ^:export exported [] 1)

(defn helper [] 2)
