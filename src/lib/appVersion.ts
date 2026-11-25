/** The version of the application (the tests do not go through the Vite config that sets it). */
export const APP_VERSION: string = typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "dev";

/** How the version is shown: `v0.1.0`, or `development` when it is not set. */
export const APP_VERSION_LABEL: string = APP_VERSION === "dev" ? "development" : `v${APP_VERSION}`;
