(ns corpus.core
  "Code that fails in the ways the trace corpus needs. Run by gen.clj.")

(defn divide [a b]
  (/ a b))

(defn wrap-once []
  (try
    (divide 1 0)
    (catch ArithmeticException e
      (throw (RuntimeException. "pricing failed" e)))))

(defn wrap-twice []
  (try
    (wrap-once)
    (catch RuntimeException e
      (throw (IllegalStateException. "checkout failed" e)))))

(defn reject [id]
  (throw (ex-info "order rejected" {:id id :reason :fraud})))

(defn wrap-ex-info []
  (try
    (reject 7)
    (catch clojure.lang.ExceptionInfo e
      (throw (ex-info "checkout failed" {:step :charge} e)))))

(defn realize []
  (doall (map (fn [x] (divide 1 x)) [1 2 0])))

(defn in-future []
  @(future (divide 1 0)))

(defn outer []
  (let [inner (fn inner [x] (divide x 0))]
    (inner 1)))

(defn anonymous []
  ((fn [] (#(divide % 0) 1))))

(defprotocol Chargeable
  (charge [this amount]))

(defrecord Card [number]
  Chargeable
  (charge [_ amount]
    (divide amount 0)))

(extend-type String
  Chargeable
  (charge [s amount]
    (divide amount (count (subs s 0 0)))))

(defmulti price :kind)

(defmethod price :book [order]
  (divide (:price order) 0))

(defmethod price :default [_]
  0)

(defn valid?->ok! [x]
  (divide x 0))

(defn *star*<=>+ [x]
  (valid?->ok! x))

(defn %pct&'quote [x]
  (*star*<=>+ x))
