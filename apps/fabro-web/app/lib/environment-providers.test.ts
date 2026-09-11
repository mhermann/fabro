import { describe, expect, test } from "bun:test";

import { EnvironmentProvider, type Environment } from "@qltysh/fabro-api-client";

import { CREATABLE_PROVIDERS, isCloneBasedEnvironment } from "./environment-providers";

function environmentWithProvider(provider: EnvironmentProvider): Environment {
  return { provider } as unknown as Environment;
}

describe("creatable environment providers", () => {
  test("every creatable provider is clone-based", () => {
    for (const provider of CREATABLE_PROVIDERS) {
      expect(isCloneBasedEnvironment(environmentWithProvider(provider))).toBe(true);
    }
  });

  test("kubernetes environments are creatable and clone-based", () => {
    expect(CREATABLE_PROVIDERS).toContain(EnvironmentProvider.KUBERNETES);
    expect(
      isCloneBasedEnvironment(environmentWithProvider(EnvironmentProvider.KUBERNETES)),
    ).toBe(true);
  });

  test("local environments are never clone-based", () => {
    expect(CREATABLE_PROVIDERS).not.toContain(EnvironmentProvider.LOCAL);
    expect(isCloneBasedEnvironment(environmentWithProvider(EnvironmentProvider.LOCAL))).toBe(
      false,
    );
  });
});
