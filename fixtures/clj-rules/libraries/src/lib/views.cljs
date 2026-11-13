(ns lib.views
  "Uses reagent (two namespaces) and react."
  (:require [reagent.core :as r]
            [reagent.dom :as rdom]
            ["react" :as react]))

(defonce counter (r/atom 0))

(defn counter-view []
  [:button {:on-click #(swap! counter inc)} @counter])

(defn mount! [el]
  (rdom/render [counter-view] el))

(defn wrapped []
  (r/as-element [:div (react/createElement "span")]))
