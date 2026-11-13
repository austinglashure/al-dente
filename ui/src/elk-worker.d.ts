// Vite's `?worker` import of elkjs's worker script: a Worker constructor.
declare module 'elkjs/lib/elk-worker.min.js?worker' {
  const ElkWorker: { new (): Worker };
  export default ElkWorker;
}
