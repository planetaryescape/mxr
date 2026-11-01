import { defineTool } from "eve/tools";
import { z } from "zod";
import { archive } from "../../fixture.ts";

export default defineTool({
  description: "Preview with dry_run true; then archive exactly those ids with the returned preview_token and dry_run false.",
  inputSchema: z.object({
    ids: z.array(z.string()),
    dry_run: z.boolean(),
    preview_token: z.string().optional(),
  }),
  approval: ({ toolInput }) => toolInput?.dry_run === true ? "not-applicable" : "user-approval",
  execute: async ({ ids, dry_run, preview_token }) => archive(ids, dry_run, preview_token),
});
