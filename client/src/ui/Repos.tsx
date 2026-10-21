/**
 * Fold-up repository panel. In an estate scene every L0 node is a repository,
 * so the checkboxes show and hide whole repositories; in a single-repository
 * scene they do the same for its top-level directories. Hiding is a client
 * filter (`filter.hiddenRoots`): the server keeps laying everything out, so
 * re-ticking a box is instant and nothing is re-ingested.
 *
 * Below the checkboxes, the scenes the server advertised in `hello` link to
 * each one (`?repo=<label>`), the current scene highlighted.
 */
import { useMemo, useState } from 'react';
import { dispatch, useCs } from '../store';

const FOLD_KEY = 'codescape.repos.folded';

function readFolded(): boolean {
  try {
    return localStorage.getItem(FOLD_KEY) === '1';
  } catch {
    return false;
  }
}

function sceneHref(label: string): string {
  const url = new URL(window.location.href);
  url.searchParams.set('repo', label);
  return url.pathname + url.search + url.hash;
}

export function Repos() {
  const model = useCs((s) => s.model);
  const hello = useCs((s) => s.hello);
  const hidden = useCs((s) => s.filter.hiddenRoots);
  const [folded, setFolded] = useState(readFolded);
  const counts = useMemo(() => {
    if (!model) return [];
    const c = new Uint32Array(model.roots.length);
    for (let i = 0; i < model.n; i++) if (model.tier[i] === 1 && model.root[i] >= 0) c[model.root[i]]++;
    return Array.from(c);
  }, [model]);
  if (!model || model.roots.length === 0) return null;
  const toggleFold = () => {
    const next = !folded;
    setFolded(next);
    try {
      localStorage.setItem(FOLD_KEY, next ? '1' : '0');
    } catch {
      /* private mode: the fold state is just not remembered */
    }
  };
  const setHidden = (roots: string[]) => dispatch({ type: 'filter', patch: { hiddenRoots: roots } });
  const toggle = (name: string) => setHidden(hidden.includes(name) ? hidden.filter((r) => r !== name) : [...hidden, name]);
  const shown = model.roots.length - hidden.filter((r) => model.roots.includes(r)).length;
  const scenes = hello?.repos ?? [];
  const current = hello?.repo ?? model.snapshot.repo;
  return (
    <div className={`cs-panel cs-repos${folded ? ' cs-folded' : ''}`} aria-label="Repositories">
      <button className="cs-fold" onClick={toggleFold} aria-expanded={!folded}>
        <span>
          Repositories <span className="cs-dim">{shown}/{model.roots.length}</span>
        </span>
        <span aria-hidden="true">{folded ? '▸' : '▾'}</span>
      </button>
      {!folded && (
        <>
          <div className="cs-row cs-small" style={{ marginTop: 6 }}>
            <button className="cs-link" onClick={() => setHidden([])} disabled={hidden.length === 0}>
              all
            </button>
            <button className="cs-link" onClick={() => setHidden([...model.roots])} disabled={shown === 0}>
              none
            </button>
          </div>
          <ul className="cs-repo-list">
            {model.roots.map((name, k) => {
              const on = !hidden.includes(name);
              return (
                <li key={name}>
                  <label className={on ? '' : 'cs-off'}>
                    <input type="checkbox" checked={on} onChange={() => toggle(name)} />
                    <span>{name}</span>
                    <span className="cs-count" title="files">
                      {counts[k]?.toLocaleString() ?? ''}
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
          {scenes.length > 1 && (
            <>
              <div className="cs-section">Scenes</div>
              <div className="cs-scenes">
                {scenes.map((label) => (
                  <a key={label} className={label === current ? 'cs-on' : ''} href={sceneHref(label)} title={`open the ${label} scene`}>
                    {label}
                  </a>
                ))}
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}
