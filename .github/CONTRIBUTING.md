# Contributing Guidelines

Guidelines and team collaboration standards for this class project.

---

## 1. Project Overview & Team Roles

<!-- TODO: Describe team members, individual responsibilities, and assignment scope -->
- **Course / Assignment:** <!-- e.g., CS XXX - Final Project -->
- **Team Members & Responsibilities:**
  - `Member 1` — <!-- e.g., Frontend / UI -->
  - `Member 2` — <!-- e.g., Backend API & Database -->
  - `Member 3` — <!-- e.g., Data Analysis & ML Models -->
  - `Member 4` — <!-- e.g., Testing, DevOps & Documentation -->

---

## 2. Environment Setup & Prerequisites

<!-- TODO: List required runtimes, tools, package managers, and setup commands -->
- **Prerequisites:** <!-- e.g., Node.js >= 18, Python >= 3.10, Docker -->
- **Installation Steps:**
  ```bash
  # TODO: Add local environment setup commands
  # git clone <repo>
  # cd smart-finance
  # npm install / pip install -r requirements.txt
  ```
- **Environment Variables:**
  <!-- TODO: Document required .env variables or provide a .env.example -->

---

## 3. Development Workflow & Branching Strategy

- **Default Branch:** `main` (should always contain working, buildable code)
- **Branch Naming Scheme:**
  - `feat/<short-description>` — New features
  - `fix/<short-description>` — Bug fixes
  - `docs/<short-description>` — Documentation changes
  - `test/<short-description>` — Test additions and updates
  - `refactor/<short-description>` — Code cleanup/restructuring

### Step-by-Step Workflow
1. Update `main` before branching: `git checkout main && git pull`
2. Create a feature branch: `git checkout -b feat/your-feature-name`
3. Make atomic, well-described commits.
4. Run tests and linting locally before pushing.
5. Push branch and open a Pull Request against `main`.

---

## 4. Code Standards & Style Guide

<!-- TODO: Define language-specific linters, formatters, and naming conventions -->
- **Formatting / Linting:** <!-- e.g., Prettier, ESLint, Black, Flake8 -->
  ```bash
  # TODO: Add commands to run linting and formatting checks
  ```
- **Code Style:**
  - Keep functions focused and modular.
  - Document key functions, modules, and API endpoints.
  - Remove debug logs and commented-out dead code before submitting PRs.

---

## 5. Testing & Verification

<!-- TODO: Define testing requirements, frameworks used, and coverage expectations -->
- **Test Frameworks:** <!-- e.g., Jest, PyTest, JUnit -->
- **Running Tests Locally:**
  ```bash
  # TODO: Add test execution command (e.g., npm test, pytest)
  ```
- **Requirements:**
  - Add unit tests for core logic and edge cases.
  - Ensure all existing and new tests pass prior to merging.

---

## 6. Pull Request & Code Review Process

- **PR Requirements:**
  - Fill out the PR template describing changes and test evidence.
  - Link any relevant issue / task cards.
  - Require at least **1 team member approval** before merging.
- **Merge Strategy:**
  - Squash and merge into `main` to maintain a clean git history.
  - Delete feature branch after merge.

---

## 7. Grading Milestones & Deliverables

<!-- TODO: Map project phases, deliverables, and due dates -->
| Milestone | Description | Target Due Date | Responsible |
|-----------|-------------|-----------------|-------------|
| Milestone 1 | <!-- e.g., Proposal & Architecture Design --> | `YYYY-MM-DD` | Team |
| Milestone 2 | <!-- e.g., Core Feature Implementation --> | `YYYY-MM-DD` | Team |
| Milestone 3 | <!-- e.g., Testing, Polish & Final Report --> | `YYYY-MM-DD` | Team |
