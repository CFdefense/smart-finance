import { Routes } from '@angular/router';
import { App } from './app';
import { UserDashboard } from '../dashboard/dashboard';
import { Login } from '../login/login';
import { UserProfile } from '../profile/profile';

export const routeConfig: Routes = [
    {path: '', redirectTo: 'login', pathMatch: 'full'},
    {path: 'login', component: Login},
    {path: 'dashboard', component: UserDashboard},
    {path: 'profile', component: UserProfile}
];
