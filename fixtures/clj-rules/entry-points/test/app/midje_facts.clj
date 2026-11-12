(ns app.midje-facts
  (:require [midje.sweet :refer [fact =>]]
            [app.core :as core]))

(fact "helper is two"
  (core/helper) => 2)
