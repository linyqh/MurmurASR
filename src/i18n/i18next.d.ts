import "i18next";
import type common from "../locales/en/common.json";
import type sidebar from "../locales/en/sidebar.json";
import type home from "../locales/en/home.json";
import type live from "../locales/en/live.json";
import type models from "../locales/en/models.json";
import type settings from "../locales/en/settings.json";
import type api from "../locales/en/api.json";
import type transcript from "../locales/en/transcript.json";
import type onboarding from "../locales/en/onboarding.json";
import type catalog from "../locales/en/catalog.json";
import type errors from "../locales/en/errors.json";

// English is the source of truth: `t()` keys are type-checked against it.
declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "common";
    resources: {
      common: typeof common;
      sidebar: typeof sidebar;
      home: typeof home;
      live: typeof live;
      models: typeof models;
      settings: typeof settings;
      api: typeof api;
      transcript: typeof transcript;
      onboarding: typeof onboarding;
      catalog: typeof catalog;
      errors: typeof errors;
    };
  }
}
