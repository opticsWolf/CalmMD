# Check-in sequence

The group checks in at departure and sends an update after choosing the ridge or valley route.

~~~mermaid
sequenceDiagram
  participant Lead as Trail lead
  participant Group as Walking group
  participant Desk as Check-in desk
  Lead->>Group: Confirm headcount
  Group-->>Lead: Everyone is here
  Lead->>Desk: Departing via the valley
  Desk-->>Lead: Update received
  Lead->>Desk: Back at the station
~~~

Try adding a participant called `Driver`. Open **Edit Connections** to send them a message from `Lead`, then apply the edit and check the preview.
