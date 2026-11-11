(ns shop.pricing-test
  (:require [clojure.test :refer [deftest is]]
            [shop.pricing :as pricing]))

(deftest food-is-taxed
  (is (< 10.9 (pricing/price {:kind :food :price 10}) 11.1)))

(deftest book-is-half-off
  (is (= 5 (pricing/price {:kind :book :price 10}))))
