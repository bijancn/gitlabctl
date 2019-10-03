# gitlabctl
Control gitlab from the command line. Currently, only one command is implemented
```
gitlabctl get environments
Retrieved 20 projects          [4.11s]
Retrieved 49 environments      [2.73s]
Retrieved environments details [7.86s]
PROJECT                     ENVIRONMENT  DEPLOYMENT           COMMIT    UPDATED
my-service-a                master       75 by bijancn        63c3655f  a week ago
my-service-a                stable       76 by bijancn        63c3655f  a week ago
my-service-a                qa           77 by bijancn        63c3655f  a week ago
my-service-a                prod         78 by bijancn        63c3655f  a week ago
my-service-b                master       142 by foo           38588be5  19 hours ago
my-service-b                stable       134 by bar           3c096e4b  a week ago
my-service-b                qa           135 by bar           3c096e4b  a week ago
my-service-b                prod         136 by bar           3c096e4b  a week ago
....
```
with the possiblity to filter for a namspace/group (and featuring colors ;)). The vision is to have a tool that allows to manipulate the Gitlab REST API as easily as `kubectl` does it for the Kubernetes API.

While the Gitlab UI is great for many things, some things are simply not there although they are available in the API. `gitlabctl` allows us to fill that gap and might also grow to become more convenient than clicking through the UI.
