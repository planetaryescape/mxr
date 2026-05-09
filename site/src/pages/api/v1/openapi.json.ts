import { readFile } from 'node:fs/promises';

export const prerender = true;

export async function GET() {
  const spec = await readFile(new URL('../../../../public/openapi.json', import.meta.url), 'utf8');

  return new Response(spec, {
    headers: {
      'content-type': 'application/json; charset=utf-8',
    },
  });
}
