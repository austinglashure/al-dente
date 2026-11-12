// The right-hand panel: what is selected, the tools for it, and its
// relations. Every repository string is rendered as React text.

import { labelFor } from './model';
import type { Child, GNode, Neighbor, ShowResult, UnitInfo } from './types';

/** Actions the panel offers; which ones appear depends on the node kind. */
export interface Tools {
  open: (n: GNode) => void;
  expandHere: (id: string) => void;
  neighbors: (id: string) => void;
  focus: (id: string) => void;
  follow: (id: string) => void;
}

/** The location of a node as `file:line:col`.
 *  Example: `where(node)` is `'src/shop/core.clj:12:1'`. */
function where(n: GNode): string {
  return n.loc ? `${n.loc.file}:${n.loc.line}:${n.loc.col}` : '';
}

/** What an aggregate's children are called.
 *  Example: `childNoun('unit')` is `'namespaces'`. */
function childNoun(kind: string): string {
  return kind === 'unit' ? 'namespaces' : kind === 'namespace' ? 'functions and vars' : 'implementations';
}

/** The details panel with its tools.
 *  Example: `<Details d={show} unit={info} tools={tools} />`. */
export function Details({ d, unit, tools }: { d: ShowResult; unit: UnitInfo | undefined; tools: Tools }) {
  const n = d.node;
  const aggregate = n.kind === 'unit' || n.kind === 'namespace';
  const relations = aggregate ? ['Used by', 'Depends on'] : ['Callers', 'Callees'];
  return (
    <section data-testid="details">
      <h2 title={n.id}>{labelFor(n)}</h2>
      <p className="muted">
        {n.kind}
        {n.subkind ? ` / ${n.subkind}` : ''}
        {(n.flags ?? []).length > 0 ? ` · ${(n.flags ?? []).join(', ')}` : ''}
      </p>
      <div className="tools">
        {aggregate && (
          <button type="button" data-action="open" onClick={() => tools.open(n)} title="Show only this on the canvas">
            Open
          </button>
        )}
        {aggregate && (
          <button type="button" data-action="expand" onClick={() => tools.expandHere(n.id)} title="Add its contents to the canvas">
            Expand here
          </button>
        )}
        {!aggregate && (
          <button type="button" data-action="neighbors" onClick={() => tools.neighbors(n.id)} title="Add callers and callees to the canvas">
            Show neighbors
          </button>
        )}
        {!aggregate && (
          <button type="button" data-action="focus" onClick={() => tools.focus(n.id)} title="Show only this and its neighbors">
            Focus
          </button>
        )}
        <button type="button" data-action="copy" onClick={() => void navigator.clipboard?.writeText(n.id)} title="Copy the full ID">
          Copy ID
        </button>
      </div>
      <p className="id">{n.id}</p>
      {unit && <UnitFacts u={unit} />}
      {n.loc && <p data-testid="location">{where(n)}</p>}
      {n.signature && <p className="mono">{n.signature}</p>}
      {n.doc && (
        <p className="doc" data-testid="doc">
          {n.doc}
        </p>
      )}
      {d.children.length > 0 && (
        <Children title={`Contains (${d.children.length})`} noun={childNoun(n.kind)} list={d.children} tools={tools} />
      )}
      <NeighborList title={relations[0] ?? ''} list={d.callers} testid="caller" onFollow={tools.follow} />
      <NeighborList title={relations[1] ?? ''} list={d.callees} testid="callee" onFollow={tools.follow} />
    </section>
  );
}

/** Build facts for a unit: files, roots, languages, service flag.
 *  Example: `<UnitFacts u={info} />` lists `deps.edn` and `services/orders/src`. */
function UnitFacts({ u }: { u: UnitInfo }) {
  return (
    <dl className="facts" data-testid="unit-facts">
      <dt>Directory</dt>
      <dd className="mono">{u.dir}</dd>
      <dt>Build files</dt>
      <dd>{u.manifests.length > 0 ? u.manifests.join(', ') : 'none (files outside any unit)'}</dd>
      <dt>Source files</dt>
      <dd>{u.files}</dd>
      <dt>Languages</dt>
      <dd>{u.langs.join(', ') || '–'}</dd>
      {u.service && (
        <>
          <dt>Service</dt>
          <dd>has an entry point</dd>
        </>
      )}
      <dt>Roots</dt>
      <dd>
        <ul>
          {u.roots.map((r) => (
            <li key={`${r.path}|${r.manifest ?? ''}|${r.alias ?? ''}`} className="mono">
              {r.path} <span className="muted">{r.kind}{r.alias ? ` :${r.alias}` : ''}</span>
            </li>
          ))}
        </ul>
      </dd>
    </dl>
  );
}

/** An aggregate's children, each selectable.
 *  Example: a namespace lists its functions. */
function Children({ title, noun, list, tools }: { title: string; noun: string; list: Child[]; tools: Tools }) {
  return (
    <>
      <h3 title={noun}>{title}</h3>
      <ul className="neighbors">
        {list.map((c) => (
          <li key={c.id}>
            <button type="button" data-testid="child" data-node-id={c.id} onClick={() => tools.follow(c.id)} title={c.id}>
              {c.name}
            </button>{' '}
            <span className="muted">{c.subkind ?? c.kind}</span>
          </li>
        ))}
      </ul>
    </>
  );
}

/** A list of neighbor links, strongest certainty first.
 *  Example: `<NeighborList title="Callees" list={d.callees} … />`. */
function NeighborList({ title, list, testid, onFollow }: { title: string; list: Neighbor[]; testid: string; onFollow: (id: string) => void }) {
  return (
    <>
      <h3>
        {title} ({list.length})
      </h3>
      <ul className="neighbors">
        {list.map((x) => (
          <li key={`${x.id}|${x.kind}|${x.reason ?? ''}`}>
            <button type="button" data-testid={testid} data-node-id={x.id} onClick={() => onFollow(x.id)} title={x.id}>
              {x.name}
            </button>{' '}
            <span className={`muted cert-${x.certainty ?? 'none'}`}>
              {x.kind}
              {x.certainty ? ` · ${x.certainty}` : ''}
              {x.reason ? ` · ${x.reason}` : ''}
              {x.count ? ` · ×${x.count}` : ''}
            </span>
          </li>
        ))}
      </ul>
    </>
  );
}
