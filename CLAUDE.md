# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AI随学 (aisuixue.com) - A React + TypeScript website deployed on Cloudflare Workers.

## Commands

```bash
# Development server with hot reload
npm run dev

# Build for production (TypeScript compile + Vite build)
npm run build

# Lint with ESLint
npm run lint

# Preview production build locally
npm run preview
```

## Deployment

The site auto-deploys to Cloudflare Workers when pushing to the `main` branch on GitHub.

- Cloudflare uses `wrangler.jsonc` config which points to `./dist` as the assets directory
- After `git push`, Cloudflare automatically runs the build and deploys

## Tech Stack

- **Framework**: React 19 + TypeScript
- **Build Tool**: Vite
- **Hosting**: Cloudflare Workers
- **Domain**: aisuixue.com / www.aisuixue.com
