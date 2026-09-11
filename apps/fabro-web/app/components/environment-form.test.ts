import { describe, expect, test } from "bun:test";

import { EnvironmentProvider } from "@qltysh/fabro-api-client";

import {
  EMPTY_ENVIRONMENT_FORM,
  createRequestFromForm,
  isEnvironmentFormValid,
  parseCreatableProvider,
  type EnvironmentFormValues,
} from "./environment-form";

function form(overrides: Partial<EnvironmentFormValues>): EnvironmentFormValues {
  return { ...EMPTY_ENVIRONMENT_FORM, id: "docker", ...overrides };
}

describe("environment image source", () => {
  test("image source requires a non-empty image reference", () => {
    expect(isEnvironmentFormValid(form({ imageSource: "image", dockerRef: "" }))).toBe(false);
    expect(
      isEnvironmentFormValid(form({ imageSource: "image", dockerRef: "ubuntu:24.04" })),
    ).toBe(true);
  });

  test("dockerfile source requires non-empty Dockerfile contents", () => {
    expect(isEnvironmentFormValid(form({ imageSource: "dockerfile", dockerfile: "" }))).toBe(false);
    expect(
      isEnvironmentFormValid(form({ imageSource: "dockerfile", dockerfile: "FROM ubuntu" })),
    ).toBe(true);
  });

  test("an empty Dockerfile does not satisfy the image-reference source", () => {
    expect(
      isEnvironmentFormValid(form({ imageSource: "image", dockerRef: "", dockerfile: "FROM x" })),
    ).toBe(false);
  });

  test("image source sends only the docker reference", () => {
    const request = createRequestFromForm(
      form({ imageSource: "image", dockerRef: "ubuntu:24.04", dockerfile: "FROM leftover" }),
    );
    expect(request.image.docker).toBe("ubuntu:24.04");
    expect(request.image.dockerfile).toBeNull();
  });

  test("dockerfile source sends only the inline Dockerfile", () => {
    const request = createRequestFromForm(
      form({ imageSource: "dockerfile", dockerRef: "leftover", dockerfile: "FROM ubuntu" }),
    );
    expect(request.image.docker).toBeNull();
    expect(request.image.dockerfile?.value).toBe("FROM ubuntu");
  });
});

describe("creatable provider parsing", () => {
  test("known creatable providers parse to themselves", () => {
    expect(parseCreatableProvider("docker")).toBe(EnvironmentProvider.DOCKER);
    expect(parseCreatableProvider("daytona")).toBe(EnvironmentProvider.DAYTONA);
    expect(parseCreatableProvider("kubernetes")).toBe(EnvironmentProvider.KUBERNETES);
  });

  test("unexpected values default to Docker", () => {
    expect(parseCreatableProvider(null)).toBe(EnvironmentProvider.DOCKER);
    expect(parseCreatableProvider("local")).toBe(EnvironmentProvider.DOCKER);
    expect(parseCreatableProvider("nonsense")).toBe(EnvironmentProvider.DOCKER);
  });
});
