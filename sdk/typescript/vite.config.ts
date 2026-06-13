import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';

/** Custom Vite plugin — serves registry schemas from ../../registry/ */
function registryPlugin(): Plugin {
  return {
    name: 'serve-registry',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (req.url !== '/api/registry_schemas') return next();

        import('fs').then((fs) => {
          const registryDir = path.resolve(__dirname, '../../registry');
          try {
            const files = fs.readdirSync(registryDir).filter((f: string) =>
              f.endsWith('.schema.json')
            );
            const schemas = files.map((f: string) => {
              const content = fs.readFileSync(path.join(registryDir, f), 'utf-8');
              return JSON.parse(content);
            });
            res.setHeader('Content-Type', 'application/json');
            res.end(JSON.stringify(schemas));
          } catch {
            res.statusCode = 500;
            res.end(JSON.stringify({ error: 'Failed to read registry' }));
          }
        });
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), registryPlugin()],
  root: '.',
  server: {
    port: 5173,
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
    },
  },
});
