import { Component } from '@angular/core';
import { RouterLink, RouterLinkActive } from '@angular/router';

/**
 * Shared navigation bar for authenticated application pages.
 *
 * The same component will be reused across Dashboard,
 * Budgets, Goals, and Rules so navigation stays consistent.
 */
@Component({
  selector: 'app-navigation',
  standalone: true,

  // RouterLink allows navigation without full page reloads.
  // RouterLinkActive marks the link for the current route.
  imports: [RouterLink, RouterLinkActive],

  templateUrl: './navigation.html',
  styleUrl: './navigation.css',
})
export class Navigation {}
