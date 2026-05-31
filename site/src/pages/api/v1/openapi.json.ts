import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { cwd } from 'node:process';

export const prerender = true;

export async function GET() {
  const spec = await readFile(join(cwd(), 'public', 'openapi.json'), 'utf8');

  return new Response(spec, {
    headers: {
      'content-type': 'application/json; charset=utf-8',
    },
  });
}
