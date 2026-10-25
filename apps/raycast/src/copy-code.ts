import { Clipboard, showHUD } from "@raycast/api";
import { BridgeError } from "./bridge/client";
import { codeSender, getFreshCodes } from "./bridge/mail";
import { message } from "./components/previewed";
import { when } from "./components/time";

/**
 * Copy the newest one-time code still in its window, with no window of its
 * own: bind it to a hotkey and paste. The clipboard entry is concealed, so
 * Raycast's clipboard history never keeps the code.
 */
export default async function CopyLatestCode() {
  try {
    await showHUD("Refreshing mail…");
    const codes = await getFreshCodes();
    const latest = codes.find((code) => code.current);
    if (!latest) {
      const last = codes[0];
      await showHUD(
        last
          ? `No current code. The last, from ${codeSender(last)}, expired ${when(last.expires_at)}`
          : "No one-time code in your mail today",
      );
      return;
    }
    await Clipboard.copy(latest.code, { concealed: true });
    await showHUD(`Copied ${latest.code} from ${codeSender(latest)}`);
  } catch (failure) {
    await showHUD(
      failure instanceof BridgeError && failure.status === 404
        ? "Update mxr to 0.6.138 or later to copy codes"
        : message(failure),
    );
  }
}
