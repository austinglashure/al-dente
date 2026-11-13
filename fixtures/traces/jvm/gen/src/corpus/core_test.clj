(ns corpus.core-test
  (:require [clojure.test :refer [deftest is]]
            [corpus.core :as core]))

(deftest divide-works
  (is (= 1 (core/divide 1 0))))
