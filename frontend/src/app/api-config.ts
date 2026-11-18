/**
 * Shared base URL for every backend API call.
 *
 * `API_URL` is not a normal variable: it is a build-time constant that the Angular
 * builder replaces with a literal string (the `define` option in angular.json).
 *  - `ng serve` / `ng test` / plain `ng build` use the default from angular.json:
 *    http://localhost:3001/api
 *  - The Docker build sets it to the relative URL `/api` (see
 *    deploy/services/frontend/Dockerfile). The browser then calls the same host
 *    that served the page, and nginx forwards /api to the backend
 *    (deploy/services/frontend/nginx.conf).
 */
declare const API_URL: string;

export const API_BASE_URL: string = API_URL;
