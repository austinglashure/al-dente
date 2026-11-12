(ns mm.shapes)

(defmulti area
  "Area of a shape map, by :shape."
  :shape)

(defmethod area :circle [{:keys [r]}] (* 3 r r))

(defmethod area :square [{:keys [s]}] (* s s))

(defmethod area :default [_] 0)

(defmulti describe (fn [x y] [(type x) (type y)]))

(defmethod describe [String String] [_ _] (str "two strings"))

(defmethod describe :default [_ _] "other")

(defmulti greet (fn [m] (:lang m)))

(defmethod greet :en [_] (str "hello"))

(defmethod greet :fr [_] (str "bonjour"))

(derive :mm.shapes/poodle :mm.shapes/dog)

(defmulti speak :animal)

(defmethod speak :mm.shapes/dog [_] (println "woof"))

(defmethod speak :default [_] nil)
