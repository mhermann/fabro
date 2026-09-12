import { describe, expect, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";

import { PullRequestChip } from "./pull-request-chip";

describe("PullRequestChip", () => {
  test("renders a linked pull request number", () => {
    let renderer: TestRenderer.ReactTestRenderer | undefined;
    act(() => {
      renderer = TestRenderer.create(
        <PullRequestChip
          number={42}
          url="https://github.com/acme/widgets/pull/42"
        />,
      );
    });

    const link = renderer!.root.findByType("a");
    const rendered = JSON.stringify(renderer!.toJSON());
    expect(link.props.href).toBe("https://github.com/acme/widgets/pull/42");
    expect(rendered).toContain("#42");
  });

  test("renders a self-hosted Forgejo instance URL", () => {
    let renderer: TestRenderer.ReactTestRenderer | undefined;
    act(() => {
      renderer = TestRenderer.create(
        <PullRequestChip
          number={7}
          url="https://git.example.com/acme/widgets/pulls/7"
        />,
      );
    });

    const link = renderer!.root.findByType("a");
    const rendered = JSON.stringify(renderer!.toJSON());
    expect(link.props.href).toBe("https://git.example.com/acme/widgets/pulls/7");
    expect(link.props.target).toBe("_blank");
    expect(link.props.rel).toBe("noreferrer");
    expect(rendered).toContain("#7");
  });
});
