import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';

import { Navigation } from '../navigation/navigation';

/**
 * Shared layout for authenticated pages.
 *
 * Displays the main navigation above the active page.
 * Child routes are rendered through RouterOutlet.
 *
 * Login and Signup will remain outside this layout
 * so users do not see financial navigation before login.
 */
@Component({
  selector: 'app-layout',
  standalone: true,
  imports: [Navigation, RouterOutlet],
  templateUrl: './app-layout.html',
})
export class AppLayout {}
