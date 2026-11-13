// Compiled by ClojureScript 1.11.60 {:target :nodejs, :nodejs-rt true, :optimizations :none}
goog.provide('app.core');
goog.require('cljs.core');
app.core.parse_qty = (function app$core$parse_qty(s){
var n = parseInt(s);
if(cljs.core.truth_(isNaN(n))){
throw (new Error(["bad qty: ",cljs.core.str.cljs$core$IFn$_invoke$arity$1(s)].join('')));
} else {
}

return n;
});
app.core.line_total = (function app$core$line_total(item){
return (new cljs.core.Keyword(null,"price","price",22129180).cljs$core$IFn$_invoke$arity$1(item) * app.core.parse_qty.call(null,new cljs.core.Keyword(null,"qty","qty",155560951).cljs$core$IFn$_invoke$arity$1(item)));
});
app.core.order_total = (function app$core$order_total(items){
return cljs.core.reduce.call(null,cljs.core._PLUS_,cljs.core.map.call(null,app.core.line_total,items));
});
app.core.valid_QMARK___GT_ok_BANG_ = (function app$core$valid_QMARK___GT_ok_BANG_(items){
return app.core.order_total.call(null,items);
});
app.core.main = (function app$core$main(){
(Error.stackTraceLimit = (50));

try{return app.core.valid_QMARK___GT_ok_BANG_.call(null,new cljs.core.PersistentVector(null, 2, 5, cljs.core.PersistentVector.EMPTY_NODE, [new cljs.core.PersistentArrayMap(null, 2, [new cljs.core.Keyword(null,"price","price",22129180),(2),new cljs.core.Keyword(null,"qty","qty",155560951),"3"], null),new cljs.core.PersistentArrayMap(null, 2, [new cljs.core.Keyword(null,"price","price",22129180),(5),new cljs.core.Keyword(null,"qty","qty",155560951),"x"], null)], null));
}catch (e528){var e = e528;
return cljs.core.println.call(null,e.stack);
}});
(cljs.core._STAR_main_cli_fn_STAR_ = app.core.main);

//# sourceMappingURL=core.js.map
