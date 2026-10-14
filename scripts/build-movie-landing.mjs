// Stage the movie site inside the main Cloudflare static-assets directory.
// Keep examples/the-why/landing as the single editable source.
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
const root = fileURLToPath(new URL('../', import.meta.url));
const source = join(root, 'examples/the-why/landing');
const destination = join(root, 'landing/the-why');
const entries = ['index.html', 'style.css', 'app.js', 'assets', 'vendor'];
function validate(file) {
  const stat = statSync(file);
  if (stat.isDirectory()) {
    for (const name of readdirSync(file)) validate(join(file, name));
  } else if (stat.size > 25 * 1024 * 1024) {
    throw new Error(`Static asset exceeds 25 MiB: ${file}`);
  }
}
for (const entry of entries) validate(join(source, entry));
if (!existsSync(join(source, 'assets/hls/master.m3u8'))) throw new Error('Generate the movie HLS assets first.');
rmSync(destination, { recursive: true, force: true });
mkdirSync(destination, { recursive: true });
for (const entry of entries) cpSync(join(source, entry), join(destination, entry), { recursive: true });
const index = join(destination, 'index.html');
writeFileSync(index, readFileSync(index, 'utf8').replace('<head>', '<head>\n<base href="/the-why/">'));
console.log('Published The Why to landing/the-why (all assets below 25 MiB).');
