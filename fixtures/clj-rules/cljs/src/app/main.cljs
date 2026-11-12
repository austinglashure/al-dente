(ns app.main
  (:require ["react" :as react]
            [reagent.core :as r]
            [app.ui :as ui])
  (:require-macros [app.macros :refer [when-dev]]))

(defn init []
  (when-dev (ui/render))
  (react/createElement "div")
  (r/atom 1))
