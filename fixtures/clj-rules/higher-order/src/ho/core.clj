(ns ho.core)

(defn inc2 [x] (+ x 2))

(defn dbl [x] (* 2 x))

(defn apply-it [f x]
  (f x))

(defn twice
  ([f] (twice f 1))
  ([f x] (f (f x))))

(defn use-apply [] (apply-it inc2 1))

(defn use-apply-2 [] (apply-it dbl 3))

(defn use-anon [] (apply-it (fn [y] y) 4))

(defn use-twice [] (twice dbl 5))

(defn use-twice-1 [] (twice inc2))

(defn mapped [xs] (map inc2 xs))

(def composed (comp inc2 dbl))

(def partial-dbl (partial dbl 2))

(def table {:inc inc2})

(defn returns-fn [] dbl)
