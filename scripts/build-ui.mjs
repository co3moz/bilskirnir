import { build } from 'esbuild';
import { mkdir, copyFile } from 'node:fs/promises';
await mkdir('web/dist', { recursive: true });
await build({ entryPoints: ['web/app.js'], bundle: true, format: 'esm', outfile: 'web/dist/app.js', minify: true, sourcemap: true, target: ['es2022'], banner: { js: '/*! Includes noVNC 1.5.0 (MPL-2.0). License: /novnc-LICENSE.txt; notices: /third-party-notices.txt. */' } });
for (const name of ['index.html', 'style.css', 'audio-worklet.js', 'mark.svg']) await copyFile(`web/${name}`, `web/dist/${name}`);
await copyFile('node_modules/@novnc/novnc/LICENSE.txt', 'web/dist/novnc-LICENSE.txt');
await copyFile('THIRD_PARTY_NOTICES.md', 'web/dist/third-party-notices.txt');
await copyFile('LICENSE', 'web/dist/LICENSE.txt');

