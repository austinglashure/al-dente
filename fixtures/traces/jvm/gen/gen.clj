;; Writes the JVM trace corpus. Run from this directory with gen.sh.
(require '[corpus.core :as c] '[clojure.java.io :as io] 'corpus.core-test 'clojure.test)

(defn trace-of [f]
  (try (f) nil
       (catch Throwable e
         (let [sw (java.io.StringWriter.)]
           (.printStackTrace e (java.io.PrintWriter. sw))
           [e (str sw)]))))

(defn write! [name text]
  (spit (io/file "../" name) text)
  (println "wrote" name))

(doseq [[name f] [["01-plain.txt" #(c/divide 1 0)]
                  ["02-nested-causes.txt" c/wrap-twice]
                  ["03-ex-info.txt" c/wrap-ex-info]
                  ["04-lazy-seq.txt" c/realize]
                  ["05-future.txt" c/in-future]
                  ["06-anonymous.txt" #(do (c/outer))]
                  ["07-anonymous-literal.txt" c/anonymous]
                  ["08-protocol-record.txt" #(c/charge (c/->Card "4242") 10)]
                  ["09-protocol-extend.txt" #(c/charge "acct" 10)]
                  ["10-multimethod.txt" #(c/price {:kind :book :price 10})]
                  ["11-munged-names.txt" #(c/%pct&'quote 1)]]]
  (write! name (second (trace-of f))))

(let [[e _] (trace-of c/wrap-ex-info)]
  (write! "14-throwable-map.edn" (with-out-str (prn (Throwable->map e))))
  (write! "15-printed-error.edn" (with-out-str (pr e))))

(write! "13-test-runner.txt"
        (with-out-str
          (binding [clojure.test/*test-out* *out*]
            (clojure.test/run-tests 'corpus.core-test))))
