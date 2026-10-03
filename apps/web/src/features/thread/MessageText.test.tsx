import { render } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { LINK_MARK_ATTRIBUTE } from "./linkHighlight";
import { MessageText } from "./MessageText";

const LINK = "https://www.camden.gov.uk/pay-council-tax";

describe("MessageText marks the link a to-do is about", () => {
  test("in the new writing", () => {
    const { container } = render(
      <MessageText
        text={`Pay here: ${LINK}`}
        showQuotes={false}
        showSignature={false}
        highlightLink={LINK}
      />,
    );
    expect(container.querySelector(`[${LINK_MARK_ATTRIBUTE}]`)).toHaveAttribute("href", LINK);
  });

  test("in quoted text and a signature once they are shown", () => {
    const text = `Thanks, paying now.\n\n> On Mon, Camden wrote:\n> Pay here: ${LINK}\n\n-- \nCamden Council\n${LINK}`;
    const { container } = render(
      <MessageText text={text} showQuotes showSignature highlightLink={LINK} />,
    );
    expect(container.querySelectorAll(`[${LINK_MARK_ATTRIBUTE}]`)).toHaveLength(2);
  });
});
