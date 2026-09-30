# Financial Budgeting App - Project Requirements

## Purpose

This document describes what the Financial Budgeting App must do (functional requirements) and how well it must do it (non-functional requirements). It is the team’s shared definition of “done.” When the team disagrees about what to build, this document is the tiebreaker, and when the team changes its mind, this document is updated first.

## Product Overview

We are building a web app that connects to a user’s bank accounts through Plaid, shows where their money goes, lets them set budgets and goals, flags unusual transactions, and answers questions about their finances through an AI chat assistant.&#32;

## Users

This app is built for people who have multiple accounts and want to see all of them in one place, as well as people who have less financial literacy and want to learn more about how their money works.&#32;

## User Requirements

Plain-language goals, written so anyone (not just developers) can check them.

- **UR-1** Users can link all their bank accounts and see them in one place.
- **UR-2** Users can see where their money goes through simple charts.
- **UR-3** Users can set budgets and savings goals.
- **UR-4** Users are warned when they go over budget or a purchase looks unusual.
- **UR-5** Users can ask an AI assistant questions about their own finances.
- **UR-6** Users can see and manage their recurring bills and subscriptions.
- **UR-7** Users have a secure account and control over their own data.

## Key Terms

- **Priority:** \[M\] must have · \[N\] nice to have · \[L\] later, if time allows.
- **Sync:** pulling new transactions from Plaid into our database.
- **Unusual transaction:** a charge more than 3× the user's usual amount for that category (starting rule; the team can tune it).
- **IDs** (e.g., FR-2.3) are referenced in GitHub issues, commits, and tests.

## Functional Requirements

### FR-1 Accounts and Sessions

- **FR-1.1** \[M\] A visitor can create an account with an email and password.
- **FR-1.2** \[M\] A user can log in and log out.
- **FR-1.3** \[M\] Pages and API routes with financial data require a logged-in session.
- **FR-1.4** \[N\] A user can view and edit their profile (name, email, password).
- **FR-1.5** \[M\] A user can delete their account and all of their data.

### FR-2 Bank Connection (Plaid)

- **FR-2.1** \[M\] A user can connect a bank through Plaid Link (Sandbox).
- **FR-2.2** \[M\] A user can see their connected accounts and balances.
- **FR-2.3** \[N\] A user can disconnect a bank.
- **FR-2.4** \[M\] The system syncs new transactions when the user presses Refresh.
- **FR-2.5** \[M\] The system syncs automatically when Plaid sends a webhook.

### FR-3 Budgets and Goals

- **FR-3.1** \[M\] A user can set a monthly budget for each spending category using a form.
- **FR-3.2** \[N\] A user can create a savings goal with a name, target amount, and target date.
- **FR-3.3** \[L\] A user can set a budget or goal by typing a sentence (e.g., “keep food under $400 a month”).

### FR-4 Monitoring and Anomaly Detection

- **FR-4.1** \[M\] After each sync, the system compares each category’s spending to its budget.
- **FR-4.2** \[N\] After each sync, the system flags unusual transactions.
- **FR-4.3** \[N\] A user can mark a flagged transaction as expected.
- **FR-4.4** \[N\] An AI agent writes a plain-English explanation for each flagged item.

### FR-5 Notifications

- **FR-5.1** \[M\] The system creates a notification when a category goes over budget or a transaction is flagged.
- **FR-5.2** \[M\] The app shows a list of notifications with an unread count.
- **FR-5.3** \[M\] A user can mark notifications as read.
- **FR-5.4** \[L\] A user can turn email notifications on or off.

### FR-6 Insights Dashboard

- **FR-6.1** \[M\] A user can see this month’s spending by category.
- **FR-6.2** \[N\] A user can see how spending changed month to month.
- **FR-6.3** \[M\] A user can see money in vs. money out for a chosen month.
- **FR-6.4** \[L\] A user can see projected spending for the rest of the month.

### FR-7 AI Chat Assistant

- **FR-7.1** \[M\] A user can ask a question about their spending, budgets, or goals and get an answer based on their own data.
- **FR-7.2** \[N\] A user can ask general money questions (e.g., “What is an emergency fund?”).
- **FR-7.3** \[N\] The chat keeps the conversation history for the current session.

### FR-8 Recurring Expenses

- **FR-8.1** \[N\] The system detects recurring charges (e.g., subscriptions, rent) from transaction history.
- **FR-8.2** \[N\] A user can see a list of recurring charges with amount and next expected date.
- **FR-8.3** \[L\] A user can mark a charge as not recurring.

## Non-Functional Requirements

Each one can be checked with a clear pass or fail.

- **NFR-1** \[M\] Passwords are hashed and never stored as plain text.
- **NFR-2** \[M\] Plaid access tokens are encrypted when stored.
- **NFR-3** \[M\] Names, emails, and account numbers are never sent to the AI provider.
- **NFR-4** \[M\] If Plaid or the AI provider is down, the app shows a clear message and the rest of the app still works.
- **NFR-5** \[N\] The dashboard loads in under 3 seconds.
- **NFR-6** \[N\] A new user can connect a bank and see their spending in under 5 minutes, with no help.

## Out of Scope

- Moving money or paying bills
- Investment or tax advice
- A native mobile app
- Real bank data (Plaid Sandbox only)
