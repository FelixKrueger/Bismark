/** Site base path, normalised to a trailing slash (respects `base` in astro.config). */
export const base = import.meta.env.BASE_URL.replace(/\/?$/, '/');

/** True for the home route. Starlight gives the index page an empty `id`. */
export const isHomePage = (route: { id: string }): boolean => route.id === '';
