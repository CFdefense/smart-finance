# Dockerfile to build the Angular frontend and serve it with nginx.
#
# Build context = frontend/ (see deploy/docker-compose.yml).
#
# Multi-stage build: stage 1 has Node and compiles the app into plain static
# files (HTML/JS/CSS); stage 2 is a small nginx image that only serves those
# files. Node and node_modules never end up in the image we run.

# ---------- Stage 1: build ----------
# Node 24 is an LTS line supported by Angular 22 (it needs ^24.15).
# "alpine" = a very small Linux base image.
FROM node:24.15-alpine AS build

WORKDIR /app

# Copy the dependency manifests FIRST and install. Docker caches each step, so
# as long as these two files don't change, this slow step is reused even when
# the source code changes. `npm ci` installs exactly what package-lock.json says.
COPY package.json package-lock.json ./
RUN npm ci

# Now the rest of the source (frontend/.dockerignore leaves out node_modules/,
# dist/, etc.).
COPY . .

# The backend URL the BROWSER will call. It is baked into the JavaScript at
# build time, so changing it means rebuilding this image.
ARG API_URL=http://localhost:3001/api

# Production build. --define replaces the API_URL constant used in
# src/app/api-config.ts with this literal string (the inner quotes make it a
# JavaScript string). Output goes to dist/frontend/browser.
RUN npx ng build --define "API_URL='${API_URL}'"

# ---------- Stage 2: serve ----------
# Pinned nginx on Alpine: a small, fast web server for static files.
FROM nginx:1.30-alpine

# Replace nginx's default site with ours (adds the single-page-app fallback).
# nginx.conf lives next to this Dockerfile, which is OUTSIDE the build context
# (frontend/), so it comes from the extra "deploy" context that
# docker-compose.yml declares under additional_contexts.
COPY --from=deploy nginx.conf /etc/nginx/conf.d/default.conf

# Copy only the built static files from the build stage into nginx's web root.
COPY --from=build /app/dist/frontend/browser /usr/share/nginx/html

# Documentation only: nginx listens on 80. Publishing happens in compose.
EXPOSE 80

# No CMD needed: the nginx image already starts nginx in the foreground.
