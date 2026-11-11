/**
 * Shared base URL for every backend API call.
 *
 * `API_URL` is not a normal variable: it is a build-time constant that the Angular
 * builder replaces with a literal string (the `define` option in angular.json).
 *  - `ng serve` / `ng test` / plain `ng build` use the default from angular.json:
 *    http://localhost:3001/api
 *  - The Docker build overrides it: `ng build --define "API_URL='<url>'"`
 *    (see deploy/services/frontend/Dockerfile).
 */
declare const API_URL: string;

export const API_BASE_URL: string = API_URL;
