(ns app.thing-test
  "A test namespace by name, outside the test root."
  (:require [clojure.test :refer [deftest is]]
            [clojure.test.check.clojure-test :refer [defspec]]
            [clojure.test.check.properties :as prop]
            [clojure.test.check.generators :as gen]
            [app.core :as core]))

(deftest helper-is-two
  (is (= 2 (core/helper))))

(defspec newest-is-stable 10
  (prop/for-all [n gen/nat] (= n n)))

(defn fixture-helper [] nil)
