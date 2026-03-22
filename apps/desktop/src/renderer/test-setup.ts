import "@testing-library/jest-dom/vitest";
import { afterAll, afterEach, beforeAll } from "vitest";
import { desktopMockServer, resetDesktopMockServer } from "./test/desktopMockServer";

beforeAll(() => {
  desktopMockServer.listen({ onUnhandledRequest: "error" });
});

afterEach(() => {
  desktopMockServer.resetHandlers();
  resetDesktopMockServer();
});

afterAll(() => {
  desktopMockServer.close();
});
