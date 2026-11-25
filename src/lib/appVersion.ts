/** The version of the application (the tests do not go through the Vite config that sets it). */
export const APP_VERSION: string = typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "dev";
