import assert from 'node:assert/strict';
import test from 'node:test';
import { classify } from './comment-ratchet.mjs';

const texts = (entries) => entries.map((e) => e.text);

test('Rust: only real comments count; strings, chars and lifetimes hide no comments', () => {
  const src = [
    'fn f<\'a>(x: &\'a str) -> char {',
    '    let url = "https://example.com // not a comment";',
    '    let raw = r#"/* not a comment */ "quoted" "#;',
    '    let quote = \'"\'; let slash = \'/\'; let esc = \'\\\'\';',
    '    /* outer /* nested */ still outer */',
    '    // counted',
    '    /// doc',
    '    //! inner doc',
    '    //// four slashes count',
    '    // SAFETY: directive',
    '    \'x\' // trailing',
    '}',
  ].join('\n');
  const r = classify('a.rs', src);
  assert.deepEqual(texts(r.counted), [
    '/* outer /* nested */ still outer */',
    '// counted',
    '//// four slashes count',
    '// trailing',
  ]);
  assert.deepEqual(texts(r.doc), ['/// doc', '//! inner doc']);
  assert.deepEqual(texts(r.directive), ['// SAFETY: directive']);
  assert.equal(r.countedLines, 3);
});

test('TS: regex, template and JSX text hide no comments; JSDoc and directives are not counted', () => {
  const src = [
    'const re = /\\/\\/ not a comment/;',
    'const t = `// not ${"/* either */"}`;',
    '/** doc */',
    'export const C = () => <a href="http://x">// text {/* counted */}</a>;',
    '// eslint-disable-next-line no-console',
    'console.log(1); // trailing',
  ].join('\n');
  const r = classify('a.tsx', src);
  assert.deepEqual(texts(r.counted), ['/* counted */', '// trailing']);
  assert.deepEqual(texts(r.doc), ['/** doc */']);
  assert.deepEqual(texts(r.directive), ['// eslint-disable-next-line no-console']);
  assert.equal(r.countedLines, 0);
});
