(ns orders.core-test
  (:require [clojure.test :refer [deftest is]]
            [orders.core :as core]))

(deftest total-sums-prices
  (is (= 3 (core/total [{:price 1} {:price 2}]))))
