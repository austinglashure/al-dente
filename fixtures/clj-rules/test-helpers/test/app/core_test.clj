(ns app.core-test
  (:require [clojure.test :refer [deftest is]]
            [app.core :as core]))

(defn make [x] (core/build x))

(defn unused-helper [x] (core/orphan x))

(def config (core/load-config))

(core/register! :test)

(deftest run-works
  (is (= 2 (core/run 1)))
  (is (= {:x 1} (make 1))))

(deftest listener-works
  (core/notify (reify core/Listener
                 (on-event [_ e] (core/send! e)))
               :hello))
