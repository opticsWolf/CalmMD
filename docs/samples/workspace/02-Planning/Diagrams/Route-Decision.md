# Route decision

Open this note in **Split** to see the source and the diagram side by side. In the Mermaid toolbar, try reconnecting `Check → Ridge` to a new node such as `Lookout`.

~~~mermaid
flowchart LR
  Start["Meet at station"] --> Check{"Clear weather?"}
  Check -->|yes| Ridge["Walk the ridge"]
  Check -.->|no| Valley["Stay in the valley"]
  Ridge --> Lunch["Lunch at the mill"]
  Valley --> Lunch
~~~

## Checkpoint sequence

This second flowchart has a subgraph and a vertical layout.

~~~mermaid
flowchart TD
  subgraph Trail["Trail checkpoints"]
    Gate["Meadow gate"] --> Bridge["Footbridge"]
    Bridge --> Mill["Old mill"]
  end
  Mill --> Return["Return to station"]
~~~

See also [Check-in sequence](Check-In-Sequence.md).
