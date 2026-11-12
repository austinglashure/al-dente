(ns app.system
  (:require [integrant.core :as ig]))

(def config
  {:app/db {}
   :app/handler {:db (ig/ref :app/db)}})

(defmethod ig/init-key :app/db [_ _] {})

(defmethod ig/init-key :app/handler [_ {:keys [db]}] (fn [_req] db))

(defmethod ig/halt-key! :app/db [_ _] nil)
