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

});
