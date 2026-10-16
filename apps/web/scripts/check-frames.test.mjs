import { describe, expect, test } from "vitest";

import {
  checkPrimary,
  checkRoute,
  importsOf,
  makeResolver,
  rendersFrame,
  routeComponent,
} from "./check-frames.mjs";

const project = (entries) => {
  const files = new Map(Object.entries(entries));
  return { files, resolve: makeResolver((candidate) => files.has(candidate)) };
};

const route = (name, component = name) =>
  `export const Route = createFileRoute("/x")({\n  component: ${component},\n});\n`;

describe("check-frames: routes", () => {
  test("a route that renders Page, PageFrame or ListWithReader passes", () => {
    for (const tag of ["Page", "PageFrame", "ListWithReader"]) {
      const { files, resolve } = project({
        "routes/x.tsx": `import { XRoute } from "@/features/x/XRoute";\n${route("XRoute")}`,
        "features/x/XRoute.tsx": `export function XRoute() { return <${tag} title="X" />; }`,
      });
      expect(checkRoute("routes/x.tsx", files, resolve)).toEqual([]);
    }
  });

  test("a route with its own bare layout is caught, with the line of its component", () => {
    const { files, resolve } = project({
      "routes/x.tsx": `import { XRoute } from "@/features/x/XRoute";\n${route("XRoute")}`,
      "features/x/XRoute.tsx": `export function XRoute() { return <div><h1>X</h1></div>; }`,
    });
    expect(checkRoute("routes/x.tsx", files, resolve)).toEqual([
      { line: 3, message: "XRoute never renders PageFrame (Page and ListWithReader do)" },
    ]);
  });

  test("a route defined in its own file is checked in that file", () => {
    const { files, resolve } = project({
      "routes/x.tsx": `${route("Local")}function Local() { return <Page title="X">y</Page>; }`,
    });
    expect(checkRoute("routes/x.tsx", files, resolve)).toEqual([]);
  });

  test("the frame may sit a few imports away, as under a thread pane", () => {
    const { files, resolve } = project({
      "routes/x.tsx": `import { Pane } from "@/features/thread/Pane";\n${route("Open")}function Open() { return <Pane />; }`,
      "features/thread/Pane.tsx": `import { Header } from "./Header";\nexport const Pane = () => <Header />;`,
      "features/thread/Header.tsx": `export const Header = () => <PageFrame title="t" />;`,
    });
    expect(checkRoute("routes/x.tsx", files, resolve)).toEqual([]);
  });

  test("a frame further away than MAX_DEPTH imports does not count", () => {
    const chain = {
      "routes/x.tsx": `import { A } from "@/a";\n${route("A")}`,
      "a.tsx": `import { B } from "@/b";`,
      "b.tsx": `import { C } from "@/c";`,
      "c.tsx": `import { D } from "@/d";`,
      "d.tsx": `import { E } from "@/e";`,
      "e.tsx": `import { F } from "@/f";`,
      "f.tsx": `export const F = () => <PageFrame title="t" />;`,
    };
    const { files, resolve } = project(chain);
    expect(checkRoute("routes/x.tsx", files, resolve)).toHaveLength(1);
  });

  test("a mention in a comment, a string or a longer tag name is not a frame", () => {
    const { files, resolve } = project({
      "a.tsx": `// <PageFrame title="x" />\n/* <Page> */\nconst a = <PageSection title="s" />; const b = <PageTabs />; const c = <Pages />;`,
    });
    expect(rendersFrame("a.tsx", files, resolve)).toBe(false);
  });

  test("redirects and inline components have nothing to check", () => {
    expect(routeComponent('createFileRoute("/x")({ beforeLoad: () => redirect() })')).toBeNull();
    expect(routeComponent('createFileRoute("/x")({ component: () => null })')).toBeNull();
    expect(routeComponent("{\n  component: Outlet,\n}")).toBe("Outlet");
  });

  test("a cycle between files ends", () => {
    const { files, resolve } = project({
      "a.tsx": `import { B } from "./b";`,
      "b.tsx": `import { A } from "./a";`,
    });
    expect(rendersFrame("a.tsx", files, resolve)).toBe(false);
  });

  test("type-only imports are not followed", () => {
    expect(importsOf('import type { A } from "./a";\nimport { B } from "./b";')).toEqual([
      { names: "{ B }", from: "./b" },
    ]);
  });
});

describe("check-frames: one primary action", () => {
  test("one button, or a branch of buttons, is one primary", () => {
    expect(checkPrimary('<PageFrame primary={<Button size="sm">New</Button>} />')).toEqual([]);
    expect(
      checkPrimary("<Page primary={busy ? <Button>Wait</Button> : <Button>Go</Button>} />"),
    ).toEqual([]);
    expect(checkPrimary("<Page primary={ready && <Button>Go</Button>} />")).toEqual([]);
    expect(
      checkPrimary("<Page primary={<Button asChild><Link to='/x'>Go</Link></Button>} />"),
    ).toEqual([]);
  });

  test("a fragment or array of actions is caught with its line", () => {
    expect(
      checkPrimary(
        "<a>\n<Page\n  primary={\n    <>\n      <Button>One</Button>\n      <Button>Two</Button>\n    </>\n  }\n/>",
      ),
    ).toEqual([
      { line: 3, message: "a frame has one primary action; move the others into overflow" },
    ]);
    expect(checkPrimary("<Page primary={[<Button key='a' />, <Link key='b' />]} />")).toHaveLength(
      1,
    );
    expect(
      checkPrimary("<Page primary={(<><Button>A</Button><Link to='/x'>B</Link></>)} />"),
    ).toHaveLength(1);
  });

  test("a fragment around one action, and braces inside strings, are fine", () => {
    expect(checkPrimary("<Page primary={<><Button>One</Button></>} />")).toEqual([]);
    expect(checkPrimary('<Page primary={<Button title="}{">x</Button>} />')).toEqual([]);
  });

  test("a prop that only ends in primary is not the slot", () => {
    expect(checkPrimary("<X notprimary={<><Button/><Button/></>} sub-primary={1} />")).toEqual([]);
  });
});
