import { eveChannel } from "eve/channels/eve";
import { httpBasic } from "eve/channels/auth";

// Synthetic-only service. The runner generates a fresh secret for each run.
export default eveChannel({
  auth: [httpBasic({ username: "spike", password: process.env.SPIKE_PASSWORD! })],
});
