(ns app.core)

(defn run [x] (inc x))

(defn build [x] {:x x})

(defn orphan [x] (dec x))

(defn load-config [] {:port 1})

(defn register! [k] k)

(defprotocol Listener
  (on-event [this e]))

(defn notify [listener e] (on-event listener e))

(defn send! [e] e)
